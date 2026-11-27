use super::*;
use crate::display::event_loop::poll;

type Queue = Presents<char, Receiver<Result<(), String>>>;

fn queue(limit: usize) -> Queue {
    Queue::new(limit, |completed| {
        completed.recv().map_err(|e| e.to_string())?
    })
    .unwrap()
}

fn submit(queue: &mut Queue, frame: char) -> Sender<Result<(), String>> {
    let (finish, completed) = channel();
    assert!(matches!(
        queue.submit(frame, |_| Ok(completed)).unwrap(),
        Completion::Pending
    ));
    finish
}

fn next(queue: &mut Queue) -> Completion<char> {
    let fd = queue.wake.as_raw_fd();
    assert!(poll(&[fd], fd, 5_000).unwrap(), "no presentation wake");
    queue.ready().unwrap()
}

#[test]
fn one_ahead_reuses_only_the_completed_slot() {
    let mut queue = queue(2);
    let b = submit(&mut queue, 'B');
    let c = submit(&mut queue, 'C');
    assert!(queue.submit('D', |_| panic!("no image free")).is_err());
    assert!(matches!(queue.ready().unwrap(), Completion::Pending));

    b.send(Ok(())).unwrap();
    assert!(matches!(next(&mut queue), Completion::Shown('B')));
    let d = submit(&mut queue, 'D');
    d.send(Ok(())).unwrap();
    c.send(Ok(())).unwrap();
    assert!(matches!(next(&mut queue), Completion::Shown('C')));
    assert!(matches!(next(&mut queue), Completion::Shown('D')));
    assert!(queue.frames.is_empty());
}

#[test]
fn two_swapchain_images_keep_one_receipt_in_flight() {
    let mut queue = queue(1);
    let b = submit(&mut queue, 'B');
    assert!(queue.submit('C', |_| panic!("no image free")).is_err());
    b.send(Ok(())).unwrap();
    assert!(matches!(next(&mut queue), Completion::Shown('B')));
    assert!(queue.can_submit());
    let rejected = queue.submit('C', |_| Err("acquire failed".into())).unwrap();
    assert!(matches!(rejected, Completion::Retry(lost) if lost == ['C']));
}

#[test]
fn rejected_ahead_submit_follows_the_earlier_real_presentation() {
    let mut queue = queue(2);
    let b = submit(&mut queue, 'B');
    assert!(matches!(
        queue.submit('C', |_| Err("acquire failed".into())).unwrap(),
        Completion::Pending
    ));
    assert!(matches!(queue.ready().unwrap(), Completion::Pending));
    b.send(Ok(())).unwrap();
    assert!(matches!(next(&mut queue), Completion::Shown('B')));
    assert!(!queue.can_submit());
    assert!(matches!(next(&mut queue), Completion::Retry(lost) if lost == ['C']));
}

#[test]
fn a_failed_front_wait_returns_both_unconfirmed_receipts() {
    let mut queue = queue(2);
    let b = submit(&mut queue, 'B');
    let c = submit(&mut queue, 'C');
    b.send(Err("panel lost".into())).unwrap();
    assert!(matches!(next(&mut queue), Completion::Retry(lost) if lost == ['B', 'C']));
    assert!(queue.observed.is_none());

    // Dropping the queue must wait for the second queued presentation to finish
    // waiting before releasing the swapchain.
    let (dropped_tx, dropped_rx) = channel();
    let teardown = thread::spawn(move || {
        drop(queue);
        let _ = dropped_tx.send(());
    });
    assert!(
        dropped_rx.recv_timeout(Duration::from_millis(30)).is_err(),
        "drop returned while the second presentation was still unwaited"
    );
    c.send(Ok(())).unwrap();
    dropped_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("drop did not finish after the second wait completed");
    teardown.join().unwrap();
}

#[test]
fn poll_delivers_readable_wake_before_reporting_peer_hangup() {
    use std::io::{Read, Write};
    use std::os::unix::net::UnixStream;

    let (mut reader, mut writer) = UnixStream::pair().unwrap();
    writer.write_all(&[1]).unwrap();
    drop(writer);

    let fd = reader.as_raw_fd();
    assert!(poll(&[fd], fd, 5_000).unwrap());
    let mut byte = [0u8; 1];
    assert_eq!(reader.read(&mut byte).unwrap(), 1);
    assert_eq!(byte, [1]);
}
