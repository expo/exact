use super::*;

pub(super) fn parse_close_payload(payload: &[u8]) -> Result<(u16, String), HostError> {
    let code = match payload.len() {
        0 => 1005,
        1 => return Err(protocol("a one-byte close")),
        _ => u16::from_be_bytes([payload[0], payload[1]]),
    };
    if code != 1005 && (!(1000..=4999).contains(&code) || matches!(code, 1004 | 1005 | 1006 | 1015))
    {
        return Err(protocol("an invalid close code"));
    }
    let reason = std::str::from_utf8(payload.get(2..).unwrap_or(&[]))
        .map_err(|_| protocol("a close reason that is not UTF-8"))?
        .to_string();
    Ok((code, reason))
}

pub(super) fn drop_controls(
    controls: &mut std::collections::VecDeque<Control>,
    command_count: &AtomicUsize,
) {
    command_count.fetch_sub(controls.len(), Ordering::AcqRel);
    controls.clear();
}

pub(super) fn replace_controls_with_close(
    controls: &mut std::collections::VecDeque<Control>,
    command_count: &AtomicUsize,
    payload: Vec<u8>,
) -> Result<(), HostError> {
    drop_controls(controls, command_count);
    reserve_control(command_count)?;
    controls.push_front(Control {
        opcode: 0x8,
        payload,
    });
    Ok(())
}

pub(super) fn reserve_control(command_count: &AtomicUsize) -> Result<(), HostError> {
    command_count
        .try_update(Ordering::AcqRel, Ordering::Acquire, |count| {
            (count < MAX_OUTBOUND_COMMANDS).then_some(count + 1)
        })
        .map(|_| ())
        .map_err(|_| HostError::Failed("the socket's outbound command queue is full".into()))
}

pub(super) fn finish_data(
    active: &mut Option<ActiveData>,
    buffered: &AtomicUsize,
    command_count: &AtomicUsize,
    state: &Mutex<SendState>,
) {
    let Some(data) = active.take() else {
        return;
    };
    if data.accounted != 0 {
        buffered.fetch_sub(data.accounted, Ordering::AcqRel);
    }
    command_count.fetch_sub(1, Ordering::AcqRel);
    let mut state = state.lock().expect("WebSocket sender poisoned");
    state.queued_bytes = state.queued_bytes.saturating_sub(data.accounted);
    state.queued_messages = state.queued_messages.saturating_sub(1);
}

pub(super) fn latch_terminal(
    result: Result<Received, HostError>,
    deliverable: bool,
    state: &Mutex<SendState>,
) {
    let peer_close = matches!(
        &result,
        Ok(Received::Closed { code, .. }) if *code != 1006
    );
    let mut send = state.lock().expect("WebSocket sender poisoned");
    send.admission = AdmissionState::Closed;
    if let Some(terminal) = send.terminal.as_mut() {
        if terminal.peer_close || !peer_close {
            #[cfg(test)]
            let became_deliverable = deliverable && !terminal.deliverable;
            terminal.deliverable |= deliverable;
            #[cfg(test)]
            if became_deliverable {
                send.history.push(TerminalStep::Deliverable);
            }
            return;
        }
    }
    #[cfg(test)]
    send.history.push(if deliverable {
        TerminalStep::LatchedDeliverable
    } else {
        TerminalStep::LatchedDeferred
    });
    send.terminal = Some(TerminalLatch {
        result,
        delivered: false,
        deliverable,
        peer_close,
    });
}

pub(super) fn deliver_terminal(
    request: &mut Option<(ReceiveRequest, ReceiveOperation)>,
    requests: &mpsc::Receiver<ReceiveRequest>,
    state: &Mutex<SendState>,
) {
    // Admission holds this same state lock through queue insertion. Once the
    // pump owns it, every request which observed OPEN is either active or
    // visible in `requests`, so a racing EOF cannot turn into channel closure.
    let delivery = {
        let mut send = state.lock().expect("WebSocket sender poisoned");
        let terminal = send.terminal.as_mut();
        let Some(terminal) =
            terminal.filter(|terminal| terminal.deliverable && !terminal.delivered)
        else {
            return;
        };
        let pending = request
            .take()
            .map(|(request, _)| request)
            .or_else(|| requests.try_recv().ok());
        terminal.delivered = pending.is_some();
        pending.map(|pending| (pending, terminal.result.clone()))
    };
    let Some((request, result)) = delivery else {
        return;
    };
    let _ = request.reply.send(result);
}

pub(super) fn defer_terminal(result: Result<Received, HostError>, state: &Mutex<SendState>) {
    latch_terminal(result, false, state);
}

pub(super) fn release_terminal(
    request: &mut Option<(ReceiveRequest, ReceiveOperation)>,
    requests: &mpsc::Receiver<ReceiveRequest>,
    state: &Mutex<SendState>,
) {
    {
        let mut send = state.lock().expect("WebSocket sender poisoned");
        // Set in the same critical section that can make the terminal
        // deliverable, so a caller dropping the socket on that result never
        // races the pump into a resetting shutdown.
        send.graceful_teardown = true;
        let (flushed, changed) = &*send.close_flushed;
        *flushed.lock().unwrap() = true;
        changed.notify_all();
        #[cfg(test)]
        send.history.push(TerminalStep::CloseSent);
        if let Some(terminal) = send.terminal.as_mut() {
            #[cfg(test)]
            let became_deliverable = !terminal.deliverable;
            terminal.deliverable = true;
            #[cfg(test)]
            if became_deliverable {
                send.history.push(TerminalStep::Deliverable);
            }
        }
    }
    deliver_terminal(request, requests, state);
}

pub(super) fn publish_terminal(
    request: &mut Option<(ReceiveRequest, ReceiveOperation)>,
    requests: &mpsc::Receiver<ReceiveRequest>,
    result: Result<Received, HostError>,
    state: &Mutex<SendState>,
) {
    latch_terminal(result, true, state);
    deliver_terminal(request, requests, state);
}

pub(super) fn fail_pump(
    request: &mut Option<(ReceiveRequest, ReceiveOperation)>,
    requests: &mpsc::Receiver<ReceiveRequest>,
    error: HostError,
    state: &Mutex<SendState>,
    shutdown: &StdTcpStream,
) {
    publish_terminal(request, requests, Err(error), state);
    let _ = shutdown.shutdown(Shutdown::Both);
}

pub(super) fn abnormal_close() -> Received {
    Received::Closed {
        code: 1006,
        reason: String::new(),
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ReadSide {
    Open,
    Closing,
    Done,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum WriteSide {
    Open,
    Done,
}

pub(super) struct CloseState {
    read: ReadSide,
    write: WriteSide,
    close_started: bool,
    close_sent: bool,
    close_received: bool,
    finish_after_close_sent: bool,
    peer_fin: bool,
}

impl CloseState {
    fn new() -> Self {
        Self {
            read: ReadSide::Open,
            write: WriteSide::Open,
            close_started: false,
            close_sent: false,
            close_received: false,
            finish_after_close_sent: false,
            peer_fin: false,
        }
    }

    fn receive_close(&mut self) {
        self.close_received = true;
        self.finish_after_close_sent = true;
        self.read = ReadSide::Done;
    }
}

#[derive(Default)]
pub(super) struct TerminalDrain {
    fragmented: Option<u8>,
    message_len: u64,
    data_left: usize,
}

pub(super) fn begin_terminal_drain(
    close: &mut CloseState,
    drain: &mut TerminalDrain,
    request: &mut Option<(ReceiveRequest, ReceiveOperation)>,
    #[cfg(test)] observer: Option<&PumpObserver>,
) {
    if close.read != ReadSide::Open {
        return;
    }
    if let Some((_, operation)) = request.as_mut() {
        if let Some((opcode, message)) = operation.message.take() {
            drain.fragmented = Some(opcode);
            drain.message_len = message.len() as u64;
        }
    }
    close.read = ReadSide::Closing;
    #[cfg(test)]
    if let Some(observer) = observer {
        observer.terminal_draining.store(true, Ordering::Release);
    }
}

pub(super) enum DrainParse {
    NeedData(usize),
    Progress,
    Closed(Received),
    TooLarge,
    Yield,
}

#[allow(clippy::too_many_arguments)]
pub(super) fn parse_terminal_drain(
    drain: &mut TerminalDrain,
    input: &mut Input,
    controls: &mut std::collections::VecDeque<Control>,
    command_count: &AtomicUsize,
    state: &Mutex<SendState>,
    limit: usize,
    close_started: bool,
    frames_left: &mut usize,
) -> Result<DrainParse, HostError> {
    let mut progressed = false;
    loop {
        if drain.data_left != 0 {
            let discarded = drain.data_left.min(input.available().len());
            input.discard(discarded);
            drain.data_left -= discarded;
            progressed |= discarded != 0;
            if drain.data_left != 0 {
                return Ok(DrainParse::NeedData(drain.data_left.min(FRAGMENT)));
            }
            *frames_left -= 1;
        }
        if *frames_left == 0 {
            return Ok(DrainParse::Yield);
        }
        let bytes = input.available();
        if bytes.len() < 2 {
            return Ok(if progressed {
                DrainParse::Progress
            } else {
                DrainParse::NeedData(2 - bytes.len())
            });
        }
        let (fin, opcode) = (bytes[0] & 0x80 != 0, bytes[0] & 0x0f);
        if bytes[0] & 0x70 != 0 || bytes[1] & 0x80 != 0 {
            return Err(protocol("reserved bits, or a masked server frame"));
        }
        let (head, length) = match bytes[1] & 0x7f {
            126 if bytes.len() < 4 => return Ok(DrainParse::NeedData(4 - bytes.len())),
            126 => (4, u16::from_be_bytes([bytes[2], bytes[3]]) as u64),
            127 if bytes.len() < 10 => return Ok(DrainParse::NeedData(10 - bytes.len())),
            127 => (10, u64::from_be_bytes(bytes[2..10].try_into().unwrap())),
            length => (2, length as u64),
        };
        if opcode >= 8 {
            if !fin || length > 125 {
                return Err(protocol("a fragmented or long control frame"));
            }
            let length = length as usize;
            let frame_length = head + length;
            if bytes.len() < frame_length {
                return Ok(DrainParse::NeedData(frame_length - bytes.len()));
            }
            input.discard(head);
            let payload = input.take(length);
            *frames_left -= 1;
            progressed = true;
            match opcode {
                0x8 => {
                    let (code, reason) = parse_close_payload(&payload)?;
                    if !close_started {
                        let echo = if code == 1005 { Vec::new() } else { payload };
                        replace_controls_with_close(controls, command_count, echo)?;
                    }
                    state.lock().expect("WebSocket sender poisoned").admission =
                        AdmissionState::Closed;
                    return Ok(DrainParse::Closed(Received::Closed { code, reason }));
                }
                0x9 if !close_started => {
                    reserve_control(command_count)?;
                    controls.push_back(Control {
                        opcode: 0xA,
                        payload,
                    });
                }
                0x9 | 0xA => {}
                _ => return Err(protocol("an unknown control opcode")),
            }
            continue;
        }
        if !matches!(opcode, 0x0..=0x2) {
            return Err(protocol("an unknown data opcode"));
        }
        match (opcode, drain.fragmented) {
            (0x1 | 0x2, None) => {
                drain.message_len = length;
                drain.fragmented = (!fin).then_some(opcode);
            }
            (0x0, Some(kind)) => {
                drain.message_len = drain.message_len.saturating_add(length);
                if fin {
                    drain.fragmented = None;
                } else {
                    drain.fragmented = Some(kind);
                }
            }
            _ => return Err(protocol("a frame out of sequence")),
        }
        if drain.message_len > limit as u64 {
            input.discard(head);
            let mut send = state.lock().expect("WebSocket sender poisoned");
            if !close_started {
                replace_controls_with_close(
                    controls,
                    command_count,
                    1009u16.to_be_bytes().to_vec(),
                )?;
                send.admission = AdmissionState::Closing;
            }
            return Ok(DrainParse::TooLarge);
        }
        if fin {
            drain.message_len = 0;
        }
        let length = usize::try_from(length)
            .map_err(|_| protocol("a frame length that does not fit this platform"))?;
        input.discard(head);
        drain.data_left = length;
        progressed = true;
        if drain.data_left == 0 {
            *frames_left -= 1;
        }
    }
}

pub(super) fn stop_polling_tcp(
    poll: &Poll,
    wire: &mut Wire,
    registered: &mut bool,
) -> Result<(), HostError> {
    if *registered {
        poll.registry()
            .deregister(wire.tcp_mut())
            .map_err(|error| {
                HostError::Failed(format!(
                    "the socket readiness deregistration failed: {error}"
                ))
            })?;
        *registered = false;
    }
    Ok(())
}

pub(super) fn poll_interests(
    poll: &Poll,
    wire: &mut Wire,
    current: &mut Interest,
    wanted: Interest,
) -> Result<bool, HostError> {
    if *current != wanted {
        poll.registry()
            .reregister(wire.tcp_mut(), SOCKET_TOKEN, wanted)
            .map_err(|error| {
                HostError::Failed(format!("the socket readiness registration failed: {error}"))
            })?;
        *current = wanted;
        return Ok(true);
    }
    Ok(false)
}

pub(super) fn abandon_active(active: &mut Option<ActiveData>, command_count: &AtomicUsize) {
    if active.take().is_some() {
        command_count.fetch_sub(1, Ordering::AcqRel);
    }
}

pub(super) fn is_close_frame(frame: &Option<OutgoingFrame>) -> bool {
    matches!(
        frame,
        Some(OutgoingFrame {
            kind: FrameKind::Control { opcode: 0x8 },
            ..
        })
    )
}

pub(super) fn discard_unsent_close_frame(
    frame: &mut Option<OutgoingFrame>,
    wire: &Wire,
    command_count: &AtomicUsize,
) {
    let unsent_close = matches!(
        frame,
        Some(OutgoingFrame {
            at: 0,
            kind: FrameKind::Control { opcode: 0x8 },
            ..
        })
    ) && !wire.wants_write();
    if unsent_close {
        frame.take();
        command_count.fetch_sub(1, Ordering::AcqRel);
    }
}

pub(super) fn drain_commands(
    commands: &mpsc::Receiver<Command>,
    controls: &mut std::collections::VecDeque<Control>,
    command_count: &AtomicUsize,
    mut retain_close: bool,
) {
    retain_close &= !controls.iter().any(|control| control.opcode == 0x8);
    while let Ok(command) = commands.try_recv() {
        match command {
            Command::Control {
                opcode: 0x8,
                payload,
            } if retain_close => {
                controls.push_back(Control {
                    opcode: 0x8,
                    payload,
                });
                retain_close = false;
            }
            Command::Data { .. } | Command::Control { .. } => {
                command_count.fetch_sub(1, Ordering::AcqRel);
            }
        }
    }
}

/// How long a finished pump keeps reading after its half-close while it waits
/// for the peer's FIN.
#[cfg(not(test))]
pub(super) const CLOSE_LINGER: Duration = Duration::from_secs(5);
#[cfg(test)]
pub(super) const CLOSE_LINGER: Duration = Duration::from_secs(2);

/// Close TCP cleanly once the WebSocket exchange is finished. Every frame the
/// pump will ever write is already in the kernel, possibly behind a slow
/// reader. Half-close writing so FIN follows those bytes, then read and discard
/// until the peer's FIN, abort, or `CLOSE_LINGER`. Shutting down the receive
/// side or closing the last handle while inbound bytes are unread instead
/// sends RST (every platform on close; Windows also on `SD_RECEIVE`), and RST
/// discards our unsent send queue — including the Close or 1009 the peer has
/// not read yet.
// @ref LLP 0059.000#312-websocket--delegating-capability-bearing-author-required — half-close and drain before teardown
#[allow(clippy::too_many_arguments)]
pub(super) fn linger_close(
    poll: &mut Poll,
    events: &mut Events,
    wire: &mut Wire,
    registered: &mut bool,
    shutdown: &StdTcpStream,
    signal: &AbortSignal,
    state: &Mutex<SendState>,
) {
    state
        .lock()
        .expect("WebSocket sender poisoned")
        .graceful_teardown = true;
    let _ = shutdown.shutdown(Shutdown::Write);
    let deadline = Instant::now() + CLOSE_LINGER;
    let interest = if *registered {
        poll.registry()
            .reregister(wire.tcp_mut(), SOCKET_TOKEN, Interest::READABLE)
    } else {
        poll.registry()
            .register(wire.tcp_mut(), SOCKET_TOKEN, Interest::READABLE)
    };
    if interest.is_ok() {
        *registered = true;
        let mut scratch = [0u8; 16 << 10];
        'linger: loop {
            if signal.aborted() {
                break;
            }
            loop {
                // A peer that keeps sending never yields WouldBlock, so the
                // cap and abort are checked after every read, not only when
                // the socket goes quiet.
                if signal.aborted() || Instant::now() >= deadline {
                    break 'linger;
                }
                match wire.tcp_mut().read(&mut scratch) {
                    Ok(0) => break 'linger,
                    Ok(_) => {}
                    Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => break,
                    Err(_) => break 'linger,
                }
            }
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                break;
            }
            match poll.poll(events, Some(left)) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
                Err(_) => break,
            }
        }
    }
    let _ = stop_polling_tcp(poll, wire, registered);
    let _ = shutdown.shutdown(Shutdown::Both);
}

#[cfg(test)]
pub(super) struct PumpExit(Option<Arc<PumpObserver>>);

#[cfg(test)]
impl Drop for PumpExit {
    fn drop(&mut self) {
        if let Some(observer) = &self.0 {
            observer.parked.store(false, Ordering::Release);
            observer.exited.store(true, Ordering::Release);
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn pump_loop(
    mut wire: Wire,
    buffered_input: Vec<u8>,
    limit: usize,
    commands: mpsc::Receiver<Command>,
    requests: mpsc::Receiver<ReceiveRequest>,
    mut poll: Poll,
    _wake: Arc<Waker>,
    shutdown: StdTcpStream,
    buffered: Arc<AtomicUsize>,
    command_count: Arc<AtomicUsize>,
    state: Arc<Mutex<SendState>>,
    signal: AbortSignal,
    #[cfg(test)] writer_gate: Option<Arc<(Mutex<bool>, std::sync::Condvar)>>,
    #[cfg(test)] pump_observer: Option<Arc<PumpObserver>>,
) {
    #[cfg(test)]
    let _exit = PumpExit(pump_observer.clone());
    #[cfg(test)]
    if let Some(observer) = &pump_observer {
        let _ = observer.send_state.set(Arc::clone(&state));
    }
    let mut events = Events::with_capacity(8);
    let mut input = Input::new(buffered_input);
    let mut terminal_drain = TerminalDrain::default();
    let mut request: Option<(ReceiveRequest, ReceiveOperation)> = None;
    let mut controls = std::collections::VecDeque::new();
    let mut active: Option<ActiveData> = None;
    let mut frame: Option<OutgoingFrame> = None;
    let mut close = CloseState::new();
    let mut read_ready = !input.available().is_empty();
    let mut write_ready = true;
    let mut tcp_registered = true;
    let mut interests = Interest::READABLE;
    let mut stall_deadline: Option<Instant> = None;

    loop {
        let mut reschedule_read = false;
        if let Err(error) = signal.check() {
            fail_pump(&mut request, &requests, error, &state, &shutdown);
            return;
        }
        if stall_deadline.is_some_and(|deadline| Instant::now() >= deadline) {
            fail_pump(
                &mut request,
                &requests,
                HostError::Failed("the socket write made no progress before its deadline".into()),
                &state,
                &shutdown,
            );
            return;
        }

        let stop_data = close.read != ReadSide::Open || close.finish_after_close_sent;
        if close.close_sent {
            drain_commands(&commands, &mut controls, &command_count, false);
        } else if stop_data {
            drain_commands(
                &commands,
                &mut controls,
                &command_count,
                !is_close_frame(&frame),
            );
        }
        if stop_data && frame.is_none() {
            abandon_active(&mut active, &command_count);
        }

        if request.is_none() {
            match requests.try_recv() {
                Ok(next) => {
                    let binary_payload = next.binary_payload;
                    request = Some((
                        next,
                        ReceiveOperation {
                            binary_payload,
                            message: None,
                        },
                    ));
                    // A readable edge may have been paused while there was no
                    // receive demand. A nonblocking read rechecks it safely.
                    read_ready = true;
                }
                Err(mpsc::TryRecvError::Empty) => {}
                Err(mpsc::TryRecvError::Disconnected) => {
                    if close.close_sent {
                        linger_close(
                            &mut poll,
                            &mut events,
                            &mut wire,
                            &mut tcp_registered,
                            &shutdown,
                            &signal,
                            &state,
                        );
                    } else {
                        let _ = shutdown.shutdown(Shutdown::Both);
                    }
                    return;
                }
            }
        }

        let mut read_wanted = 0;
        let mut read_frames = MAX_READ_FRAMES_PER_TURN;
        if close.read == ReadSide::Open {
            if let Some((_, operation)) = request.as_mut() {
                match parse_available(
                    operation,
                    &mut input,
                    &mut controls,
                    &command_count,
                    &state,
                    limit,
                    close.close_started,
                    &mut read_frames,
                ) {
                    Ok(Parse::Received(received)) => {
                        match received {
                            received @ Received::Closed { .. } => {
                                close.receive_close();
                                #[cfg(test)]
                                if let Some(observer) = &pump_observer {
                                    observer.close_received.store(true, Ordering::Release);
                                }
                                if close.close_sent || close.write == WriteSide::Done {
                                    publish_terminal(&mut request, &requests, Ok(received), &state);
                                } else {
                                    defer_terminal(Ok(received), &state);
                                }
                            }
                            Received::TooLarge => {
                                discard_unsent_close_frame(&mut frame, &wire, &command_count);
                                close.read = ReadSide::Done;
                                close.finish_after_close_sent = true;
                                if close.close_sent || close.write == WriteSide::Done {
                                    publish_terminal(
                                        &mut request,
                                        &requests,
                                        Ok(Received::TooLarge),
                                        &state,
                                    );
                                } else {
                                    defer_terminal(Ok(Received::TooLarge), &state);
                                }
                            }
                            received => {
                                let (finished, _) = request.take().unwrap();
                                let _ = finished.reply.send(Ok(received));
                            }
                        }
                        reschedule_read = true;
                    }
                    Ok(Parse::NeedData(wanted)) => read_wanted = wanted,
                    Ok(Parse::Yield) => reschedule_read = true,
                    Err(error) => {
                        fail_pump(&mut request, &requests, error, &state, &shutdown);
                        return;
                    }
                }
            }
        }

        if close.read == ReadSide::Closing && !reschedule_read {
            match parse_terminal_drain(
                &mut terminal_drain,
                &mut input,
                &mut controls,
                &command_count,
                &state,
                limit,
                close.close_started,
                &mut read_frames,
            ) {
                Ok(DrainParse::NeedData(wanted)) => read_wanted = wanted,
                Ok(DrainParse::Progress | DrainParse::Yield) => reschedule_read = true,
                Ok(DrainParse::Closed(received)) => {
                    close.receive_close();
                    #[cfg(test)]
                    if let Some(observer) = &pump_observer {
                        observer.close_received.store(true, Ordering::Release);
                    }
                    if close.close_sent || close.write == WriteSide::Done {
                        publish_terminal(&mut request, &requests, Ok(received), &state);
                    } else {
                        defer_terminal(Ok(received), &state);
                    }
                    reschedule_read = true;
                }
                Ok(DrainParse::TooLarge) => {
                    discard_unsent_close_frame(&mut frame, &wire, &command_count);
                    close.read = ReadSide::Done;
                    close.finish_after_close_sent = true;
                    if close.close_sent || close.write == WriteSide::Done {
                        publish_terminal(&mut request, &requests, Ok(Received::TooLarge), &state);
                    } else {
                        defer_terminal(Ok(Received::TooLarge), &state);
                    }
                    reschedule_read = true;
                }
                Err(error) => {
                    fail_pump(&mut request, &requests, error, &state, &shutdown);
                    return;
                }
            }
        }

        let background_read = close.read == ReadSide::Closing
            || close.peer_fin
            || close.close_sent
            || command_count.load(Ordering::Acquire) != 0
            || frame.is_some()
            || wire.wants_write()
            || active.is_some();
        if request.is_none() && close.read == ReadSide::Open && !reschedule_read && background_read
        {
            let mut frames = MAX_READ_FRAMES_PER_TURN;
            match parse_control_prefix(
                &mut input,
                &mut controls,
                &command_count,
                &state,
                close.close_started,
                &mut frames,
            ) {
                Ok(ControlParse::NeedData(wanted)) => read_wanted = wanted,
                Ok(ControlParse::Data) => {
                    // Preserve receive backpressure and the binary
                    // head-only contract until demand is posted.
                    read_ready = false;
                }
                Ok(ControlParse::Progress | ControlParse::Yield) => {
                    reschedule_read = true;
                }
                Ok(ControlParse::Closed(received)) => {
                    close.receive_close();
                    #[cfg(test)]
                    if let Some(observer) = &pump_observer {
                        observer.close_received.store(true, Ordering::Release);
                    }
                    if close.close_sent || close.write == WriteSide::Done {
                        publish_terminal(&mut request, &requests, Ok(received), &state);
                    } else {
                        defer_terminal(Ok(received), &state);
                    }
                    reschedule_read = true;
                }
                Err(error) => {
                    fail_pump(&mut request, &requests, error, &state, &shutdown);
                    return;
                }
            }
        }

        let should_read = read_ready
            && close.read != ReadSide::Done
            && !reschedule_read
            && (request.is_some() || background_read);
        if should_read {
            let wanted = read_wanted.clamp(1, FRAGMENT);
            match wire.read_into(&mut input, wanted) {
                Ok(ReadProgress::Bytes) => reschedule_read = true,
                Ok(ReadProgress::Blocked) if close.read == ReadSide::Open => read_ready = false,
                Ok(ReadProgress::Blocked | ReadProgress::Eof) | Err(_) => {
                    if signal.aborted() {
                        fail_pump(
                            &mut request,
                            &requests,
                            signal.check().unwrap_err(),
                            &state,
                            &shutdown,
                        );
                        return;
                    }
                    close.read = ReadSide::Done;
                    if !close.close_received {
                        let admitted_close =
                            state.lock().expect("WebSocket sender poisoned").admission
                                == AdmissionState::Closing;
                        if admitted_close && !close.close_sent && close.write == WriteSide::Open {
                            defer_terminal(Ok(abnormal_close()), &state);
                        } else {
                            publish_terminal(&mut request, &requests, Ok(abnormal_close()), &state);
                        }
                    }
                    reschedule_read = true;
                }
            }
        }

        // A write-side close is independent of a peer FIN. Give readable work
        // above one turn to retain a Close, then stop because no reply can be
        // put on the wire.
        if close.write == WriteSide::Done {
            begin_terminal_drain(
                &mut close,
                &mut terminal_drain,
                &mut request,
                #[cfg(test)]
                pump_observer.as_deref(),
            );
            if close.read == ReadSide::Closing && !close.close_received {
                continue;
            }
            publish_terminal(&mut request, &requests, Ok(abnormal_close()), &state);
            let _ = stop_polling_tcp(&poll, &mut wire, &mut tcp_registered);
            let _ = shutdown.shutdown(Shutdown::Both);
            return;
        }

        let stop_data = close.read != ReadSide::Open || close.finish_after_close_sent;
        if stop_data && frame.is_none() {
            abandon_active(&mut active, &command_count);
        }
        if frame.is_none() && !wire.wants_write() && !close.close_sent {
            if let Some(control) = controls.pop_front() {
                match masked_frame(true, control.opcode, &control.payload) {
                    Ok(bytes) => {
                        frame = Some(OutgoingFrame {
                            bytes,
                            at: 0,
                            kind: FrameKind::Control {
                                opcode: control.opcode,
                            },
                        });
                    }
                    Err(error) => {
                        fail_pump(
                            &mut request,
                            &requests,
                            HostError::Failed(error.to_string()),
                            &state,
                            &shutdown,
                        );
                        return;
                    }
                }
            } else if !stop_data {
                if active.is_none() {
                    match commands.try_recv() {
                        Ok(Command::Data {
                            opcode,
                            payload,
                            accounted,
                        }) => {
                            active = Some(ActiveData {
                                opcode,
                                payload,
                                accounted,
                                next: 0,
                            });
                        }
                        Ok(Command::Control { opcode, payload }) => {
                            match masked_frame(true, opcode, &payload) {
                                Ok(bytes) => {
                                    frame = Some(OutgoingFrame {
                                        bytes,
                                        at: 0,
                                        kind: FrameKind::Control { opcode },
                                    });
                                }
                                Err(error) => {
                                    fail_pump(
                                        &mut request,
                                        &requests,
                                        HostError::Failed(error.to_string()),
                                        &state,
                                        &shutdown,
                                    );
                                    return;
                                }
                            }
                        }
                        Err(mpsc::TryRecvError::Empty | mpsc::TryRecvError::Disconnected) => {}
                    }
                }
                if frame.is_none() {
                    if let Some(data) = active.as_mut() {
                        match data.frame() {
                            Ok((bytes, final_fragment)) => {
                                frame = Some(OutgoingFrame {
                                    bytes,
                                    at: 0,
                                    kind: FrameKind::Data { final_fragment },
                                });
                            }
                            Err(error) => {
                                fail_pump(
                                    &mut request,
                                    &requests,
                                    HostError::Failed(error.to_string()),
                                    &state,
                                    &shutdown,
                                );
                                return;
                            }
                        }
                    }
                }
            }
        }

        let writes_allowed = {
            #[cfg(test)]
            {
                writer_gate
                    .as_ref()
                    .is_none_or(|gate| *gate.0.lock().expect("WebSocket writer gate poisoned"))
            }
            #[cfg(not(test))]
            {
                true
            }
        };
        let pending_before_write = frame.is_some() || wire.wants_write();
        if pending_before_write {
            stall_deadline.get_or_insert_with(|| Instant::now() + WRITE_STALL_TIMEOUT);
        }
        let mut retry_write = false;
        if writes_allowed {
            if let Some(outgoing) = frame.as_mut() {
                let can_attempt = match &wire {
                    Wire::Plain(_) => write_ready,
                    Wire::Tls { conn, .. } => !conn.wants_write() || write_ready,
                };
                let close_frame = matches!(&outgoing.kind, FrameKind::Control { opcode: 0x8 });
                let at_before = outgoing.at;
                match can_attempt.then(|| wire.write_frame(outgoing)).transpose() {
                    Ok(None) => {}
                    Ok(Some(WriteProgress::Eof)) => {
                        close.write = WriteSide::Done;
                        begin_terminal_drain(
                            &mut close,
                            &mut terminal_drain,
                            &mut request,
                            #[cfg(test)]
                            pump_observer.as_deref(),
                        );
                        defer_terminal(Ok(abnormal_close()), &state);
                    }
                    Ok(Some(WriteProgress::Network(_bytes))) => {
                        let progressed_at = Instant::now();
                        stall_deadline = Some(progressed_at + WRITE_STALL_TIMEOUT);
                        retry_write = true;
                        #[cfg(test)]
                        if let Some(observer) = &pump_observer {
                            observer.network_bytes.fetch_add(_bytes, Ordering::AcqRel);
                            observer.write_blocked.store(false, Ordering::Release);
                            *observer
                                .last_write_progress
                                .lock()
                                .expect("WebSocket progress observer poisoned") =
                                Some(progressed_at);
                        }
                    }
                    Ok(Some(WriteProgress::Retry)) => retry_write = true,
                    Ok(Some(WriteProgress::Blocked)) => {
                        write_ready = false;
                        #[cfg(test)]
                        if let Some(observer) = &pump_observer {
                            observer.write_blocked.store(true, Ordering::Release);
                        }
                    }
                    Err(error) => {
                        close.write = WriteSide::Done;
                        begin_terminal_drain(
                            &mut close,
                            &mut terminal_drain,
                            &mut request,
                            #[cfg(test)]
                            pump_observer.as_deref(),
                        );
                        defer_terminal(
                            Err(HostError::Failed(format!(
                                "the socket write failed: {error}"
                            ))),
                            &state,
                        );
                    }
                }
                if close_frame && outgoing.at != at_before {
                    close.close_started = true;
                }
                if close.write == WriteSide::Open && wire.frame_flushed(outgoing) {
                    let completed = frame.take().unwrap();
                    match completed.kind {
                        FrameKind::Data { final_fragment } if final_fragment => {
                            finish_data(&mut active, &buffered, &command_count, &state);
                        }
                        FrameKind::Data { .. } if stop_data => {
                            abandon_active(&mut active, &command_count);
                        }
                        FrameKind::Data { .. } => {}
                        FrameKind::Control { opcode: 0x8 } => {
                            command_count.fetch_sub(1, Ordering::AcqRel);
                            close.close_started = true;
                            close.close_sent = true;
                            drop_controls(&mut controls, &command_count);
                            abandon_active(&mut active, &command_count);
                            drain_commands(&commands, &mut controls, &command_count, false);
                            release_terminal(&mut request, &requests, &state);
                        }
                        FrameKind::Control { .. } => {
                            command_count.fetch_sub(1, Ordering::AcqRel);
                        }
                    }
                    retry_write = true;
                }
            } else if wire.wants_write() && write_ready {
                match wire.write_tls_pending() {
                    Ok(WriteProgress::Eof) => {
                        close.write = WriteSide::Done;
                        begin_terminal_drain(
                            &mut close,
                            &mut terminal_drain,
                            &mut request,
                            #[cfg(test)]
                            pump_observer.as_deref(),
                        );
                        defer_terminal(Ok(abnormal_close()), &state);
                    }
                    Ok(WriteProgress::Network(_bytes)) => {
                        let progressed_at = Instant::now();
                        stall_deadline = Some(progressed_at + WRITE_STALL_TIMEOUT);
                        retry_write = true;
                        #[cfg(test)]
                        if let Some(observer) = &pump_observer {
                            observer.network_bytes.fetch_add(_bytes, Ordering::AcqRel);
                            observer.write_blocked.store(false, Ordering::Release);
                            *observer
                                .last_write_progress
                                .lock()
                                .expect("WebSocket progress observer poisoned") =
                                Some(progressed_at);
                        }
                    }
                    Ok(WriteProgress::Retry) => retry_write = true,
                    Ok(WriteProgress::Blocked) => {
                        write_ready = false;
                        #[cfg(test)]
                        if let Some(observer) = &pump_observer {
                            observer.write_blocked.store(true, Ordering::Release);
                        }
                    }
                    Err(error) => {
                        close.write = WriteSide::Done;
                        begin_terminal_drain(
                            &mut close,
                            &mut terminal_drain,
                            &mut request,
                            #[cfg(test)]
                            pump_observer.as_deref(),
                        );
                        defer_terminal(
                            Err(HostError::Failed(format!(
                                "the socket write failed: {error}"
                            ))),
                            &state,
                        );
                    }
                }
            }
        }

        if close.write == WriteSide::Done {
            begin_terminal_drain(
                &mut close,
                &mut terminal_drain,
                &mut request,
                #[cfg(test)]
                pump_observer.as_deref(),
            );
            if close.read == ReadSide::Closing && !close.close_received {
                continue;
            }
            publish_terminal(&mut request, &requests, Ok(abnormal_close()), &state);
            let _ = stop_polling_tcp(&poll, &mut wire, &mut tcp_registered);
            let _ = shutdown.shutdown(Shutdown::Both);
            return;
        }
        let pending_write = frame.is_some() || wire.wants_write();
        if !pending_write {
            stall_deadline = None;
        }
        if (close.finish_after_close_sent && close.close_sent && !pending_write)
            || (close.read == ReadSide::Done
                && !pending_write
                && controls.is_empty()
                && command_count.load(Ordering::Acquire) == 0)
        {
            linger_close(
                &mut poll,
                &mut events,
                &mut wire,
                &mut tcp_registered,
                &shutdown,
                &signal,
                &state,
            );
            return;
        }

        // Read/parse work is bounded. Writes and their no-progress deadline
        // get a turn before another local input batch.
        if reschedule_read || retry_write {
            continue;
        }
        if command_count.load(Ordering::Acquire) != 0 && !pending_write {
            continue;
        }
        // Windows rearms readiness after a read returns WouldBlock. Watching
        // an always-writable socket with no output makes an idle receive spin.
        let wanted = if close.read == ReadSide::Done {
            Interest::WRITABLE
        } else if pending_write {
            Interest::READABLE.add(Interest::WRITABLE)
        } else {
            Interest::READABLE
        };
        match poll_interests(&poll, &mut wire, &mut interests, wanted) {
            Ok(true) if pending_write => {
                // mio readiness is edge-triggered on epoll. The socket may
                // already be writable when EPOLL_CTL_MOD changes the
                // interest set, so probe it before waiting for an edge
                // which need not recur.
                write_ready = true;
                continue;
            }
            Ok(_) => {}
            Err(error) => {
                fail_pump(&mut request, &requests, error, &state, &shutdown);
                return;
            }
        }
        #[cfg(test)]
        if let Some(observer) = &pump_observer {
            observer
                .receiving
                .store(request.is_some(), Ordering::Release);
            observer.parked.store(true, Ordering::Release);
        }
        let timeout =
            stall_deadline.map(|deadline| deadline.saturating_duration_since(Instant::now()));
        let waited = poll.poll(&mut events, timeout);
        #[cfg(test)]
        if let Some(observer) = &pump_observer {
            observer.parked.store(false, Ordering::Release);
            observer.returns.fetch_add(1, Ordering::AcqRel);
        }
        match waited {
            Ok(()) => {
                // Waker and socket readiness can be coalesced. After every
                // wake, retry nonblocking I/O and drain it to WouldBlock so a
                // readiness edge cannot be consumed only by the poll call.
                read_ready = true;
                write_ready = true;
                for event in &events {
                    if event.token() != SOCKET_TOKEN {
                        continue;
                    }
                    read_ready |= event.is_readable();
                    write_ready |= event.is_writable();
                    if event.is_read_closed() && close.read != ReadSide::Done {
                        // A peer FIN is an in-order end of the byte stream,
                        // not a terminal condition: data frames ahead of it
                        // still belong to receive demand. Only parse leading
                        // control frames without demand; EOF itself is
                        // processed when a read reaches it.
                        // @ref LLP 0059.000#312-websocket--delegating-capability-bearing-author-required — read FIN keeps queued messages
                        close.peer_fin = true;
                        read_ready = true;
                        #[cfg(test)]
                        if let Some(observer) = &pump_observer {
                            observer.peer_fin.store(true, Ordering::Release);
                        }
                    }
                    if event.is_write_closed() {
                        close.write = WriteSide::Done;
                        begin_terminal_drain(
                            &mut close,
                            &mut terminal_drain,
                            &mut request,
                            #[cfg(test)]
                            pump_observer.as_deref(),
                        );
                        read_ready = true;
                    }
                    if event.is_error() {
                        // The next nonblocking operation obtains the concrete
                        // error without treating a read HUP as a write HUP.
                        read_ready = true;
                        write_ready = true;
                    }
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) => {
                fail_pump(
                    &mut request,
                    &requests,
                    HostError::Failed(format!("the socket readiness wait failed: {error}")),
                    &state,
                    &shutdown,
                );
                return;
            }
        }
    }
}

pub(super) fn masked_frame(fin: bool, opcode: u8, payload: &[u8]) -> std::io::Result<Vec<u8>> {
    let mut mask = [0u8; 4];
    getrandom::getrandom(&mut mask).map_err(|error| std::io::Error::other(error.to_string()))?;
    let mut frame = Vec::with_capacity(payload.len().saturating_add(14));
    frame.push((if fin { 0x80 } else { 0 }) | opcode);
    match payload.len() {
        length if length < 126 => frame.push(0x80 | length as u8),
        length if length <= u16::MAX as usize => {
            frame.push(0x80 | 126);
            frame.extend_from_slice(&(length as u16).to_be_bytes());
        }
        length => {
            frame.push(0x80 | 127);
            frame.extend_from_slice(&(length as u64).to_be_bytes());
        }
    }
    frame.extend_from_slice(&mask);
    frame.extend(
        payload
            .iter()
            .enumerate()
            .map(|(index, byte)| byte ^ mask[index % 4]),
    );
    Ok(frame)
}

#[derive(Clone)]
pub(super) enum Received {
    Text(String),
    Binary(Vec<u8>),
    BinaryLength(usize),
    TooLarge,
    Closed { code: u16, reason: String },
}

impl Socket {
    pub(super) fn receive(&mut self, binary_payload: bool) -> Result<Received, HostError> {
        self.signal.check()?;
        let mut state = self.sender.state.lock().expect("WebSocket sender poisoned");
        if let Some(terminal) = state.terminal.as_mut() {
            if terminal.deliverable && !terminal.delivered {
                terminal.delivered = true;
                return terminal.result.clone();
            }
        }
        if state.admission == AdmissionState::Closed
            && state
                .terminal
                .as_ref()
                .is_none_or(|terminal| terminal.deliverable)
        {
            return Ok(abnormal_close());
        }
        let (reply, received) = mpsc::sync_channel(1);
        if self
            .requests
            .send(ReceiveRequest {
                binary_payload,
                reply,
            })
            .is_err()
        {
            state.admission = AdmissionState::Closed;
            return Ok(abnormal_close());
        }
        if let Err(error) = self.wake.wake() {
            state.admission = AdmissionState::Closed;
            drop(state);
            let _ = self.shutdown.shutdown(Shutdown::Both);
            return Err(HostError::Failed(format!(
                "the socket wake notification failed: {error}"
            )));
        }
        drop(state);
        match received.recv() {
            Ok(result) => result,
            Err(_) => {
                self.signal.check()?;
                let mut state = self.sender.state.lock().expect("WebSocket sender poisoned");
                if let Some(terminal) = state.terminal.as_mut() {
                    if terminal.deliverable && !terminal.delivered {
                        terminal.delivered = true;
                        return terminal.result.clone();
                    }
                }
                Ok(abnormal_close())
            }
        }
    }
}

impl MessageSource for Socket {
    fn next(&mut self) -> Result<Incoming, HostError> {
        Ok(match self.receive(false)? {
            Received::Text(text) => Incoming::Text(text),
            Received::Binary(bytes) => Incoming::Binary(bytes.len()),
            Received::BinaryLength(length) => Incoming::Binary(length),
            Received::TooLarge => Incoming::TooLarge,
            Received::Closed { code, reason } => Incoming::Closed { code, reason },
        })
    }

    fn next_event(&mut self) -> Result<Event, HostError> {
        Ok(match self.receive(true)? {
            Received::Text(text) => Event::Message(Message::Text(text)),
            Received::Binary(bytes) => Event::Message(Message::Binary(bytes)),
            Received::BinaryLength(_) => unreachable!("event receive requested binary bytes"),
            Received::TooLarge => {
                return Err(HostError::Failed(
                    "the socket message exceeded its configured limit".into(),
                ));
            }
            Received::Closed { code, reason } => Event::Close {
                code,
                reason,
                was_clean: code != 1006,
            },
        })
    }

    fn sender(&self) -> Option<Arc<dyn MessageSender>> {
        Some(self.sender.clone())
    }

    fn protocol(&self) -> &str {
        &self.protocol
    }
}

impl Drop for Socket {
    fn drop(&mut self) {
        if self.sender.admission() == AdmissionState::Open && !self.signal.aborted() {
            let _ = self.sender.close(Some(1000), "");
        }
        self.sender.mark_closed();
        // After its Close reaches TCP the pump half-closes and finishes TCP
        // itself; a receive-side shutdown here could reset those queued
        // final bytes.
        let graceful = self
            .sender
            .state
            .lock()
            .expect("WebSocket sender poisoned")
            .graceful_teardown;
        if !graceful {
            let _ = self.shutdown.shutdown(Shutdown::Both);
        }
        let _ = self.wake.wake();
    }
}
