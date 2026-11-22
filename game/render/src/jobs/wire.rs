use super::{Cancel, Job, JobError};
use std::any::TypeId;

/// A presentation job whose inputs and successful output can cross a worker boundary.
pub trait WireJob: Job + Sized {
    /// Unique name in the module's remote job table.
    const NAME: &'static str;
    /// Append the encoded input.
    fn encode(&self, out: &mut Vec<u8>);
    /// Read a complete encoded input.
    fn decode(bytes: &[u8]) -> Result<Self, JobError>;
    /// Append the encoded successful output.
    fn encode_output(output: &Self::Output, out: &mut Vec<u8>);
    /// Read a complete encoded successful output.
    fn decode_output(bytes: &[u8]) -> Result<Self::Output, JobError>;
    /// Whether this job value can run on a worker.
    fn wire(&self) -> bool {
        true
    }
}

/// Boundaries of the job's own run, excluding either codec.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RunMark {
    /// About to call the job.
    Begin,
    /// The job returned.
    End,
}

const RESULT: u8 = 0;
const JOB_ERROR: u8 = 1;

/// Failure to install a transport or dispatch an input to a registered job.
#[derive(Debug, PartialEq, Eq)]
pub enum WireError {
    /// This owner thread already has a transport.
    AlreadyInstalled,
    /// More than one job entry has the same name.
    DuplicateName(&'static str),
    /// The input names no registered job.
    UnknownName,
    /// The registered job could not decode the input.
    DecodeInput(JobError),
}

type Dispatch = fn(&[u8], &mut dyn FnMut(RunMark)) -> Result<Vec<u8>, WireError>;

/// An entry in the GPU module's remote job table (LLP 1182.000 D9).
pub struct RemoteJob {
    /// Name sent over the wire.
    pub name: &'static str,
    type_id: fn() -> TypeId,
    dispatch: Dispatch,
}

impl RemoteJob {
    /// Register a job without requiring a value of its type.
    pub const fn of<J: WireJob>() -> Self {
        Self {
            name: J::NAME,
            type_id: TypeId::of::<J>,
            dispatch: dispatch::<J>,
        }
    }

    pub(super) fn is<J: Job>(&self) -> bool {
        (self.type_id)() == TypeId::of::<J>()
    }
}

pub(super) fn check_table(entries: &[RemoteJob]) -> Result<(), WireError> {
    for (index, entry) in entries.iter().enumerate() {
        if entries[..index]
            .iter()
            .any(|other| other.name == entry.name)
        {
            return Err(WireError::DuplicateName(entry.name));
        }
    }
    Ok(())
}

/// Dispatch an encoded job from the generated GPU module's worker export.
/// The first response byte is 0 for a result
/// or 1 for a job error with a UTF-8 message. Dispatch failures have no response bytes.
pub fn run_remote_job(
    entries: &[RemoteJob],
    name: &str,
    input: &[u8],
    mut mark: impl FnMut(RunMark),
) -> Result<Vec<u8>, WireError> {
    check_table(entries)?;
    let entry = entries
        .iter()
        .find(|entry| entry.name == name)
        .ok_or(WireError::UnknownName)?;
    (entry.dispatch)(input, &mut mark)
}

fn dispatch<J: WireJob>(input: &[u8], mark: &mut dyn FnMut(RunMark)) -> Result<Vec<u8>, WireError> {
    let job = J::decode(input).map_err(WireError::DecodeInput)?;
    let cancel = Cancel::default();
    mark(RunMark::Begin);
    let result = job.run(&cancel);
    mark(RunMark::End);
    match result {
        Ok(output) => {
            let mut bytes = vec![RESULT];
            J::encode_output(&output, &mut bytes);
            Ok(bytes)
        }
        Err(JobError(message)) => {
            let mut bytes = Vec::with_capacity(1 + message.len());
            bytes.push(JOB_ERROR);
            bytes.extend(message.into_bytes());
            Ok(bytes)
        }
    }
}

type DecodeOutput<O> = fn(&[u8]) -> Result<O, JobError>;

pub(super) struct WireCodec<J: Job> {
    pub wire: fn(&J) -> bool,
    pub encode: fn(&J, &mut Vec<u8>),
    decode_output: DecodeOutput<J::Output>,
    #[cfg(debug_assertions)]
    decode: fn(&[u8]) -> Result<J, JobError>,
    #[cfg(debug_assertions)]
    encode_output: fn(&J::Output, &mut Vec<u8>),
}

impl<J: WireJob> WireCodec<J> {
    pub(super) fn new() -> Self {
        Self {
            wire: J::wire,
            encode: J::encode,
            decode_output: J::decode_output,
            #[cfg(debug_assertions)]
            decode: J::decode,
            #[cfg(debug_assertions)]
            encode_output: J::encode_output,
        }
    }
}

impl<J: Job> WireCodec<J> {
    pub(super) fn read_answer(&self, bytes: &[u8]) -> Option<Result<J::Output, JobError>> {
        let (&tag, payload) = bytes.split_first()?;
        match tag {
            RESULT => (self.decode_output)(payload).ok().map(Ok),
            JOB_ERROR => std::str::from_utf8(payload)
                .ok()
                .map(|message| Err(JobError(message.to_owned()))),
            _ => None,
        }
    }

    #[cfg(debug_assertions)]
    pub(super) fn roundtrip(&self, job: J, cancel: &Cancel) -> Result<J::Output, JobError> {
        let mut bytes = Vec::new();
        (self.encode)(&job, &mut bytes);
        let job = (self.decode)(&bytes).expect("remote job could not decode its input");
        let output = job.run(cancel)?;
        bytes.clear();
        (self.encode_output)(&output, &mut bytes);
        Ok((self.decode_output)(&bytes).expect("remote job could not decode its output"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    thread_local! {
        static PHASE: Cell<u8> = const { Cell::new(0) };
    }

    struct Observed;

    impl Job for Observed {
        type Output = [u8; 2];

        fn bytes(&self) -> usize {
            0
        }

        fn output_bytes(_: &Self::Output) -> usize {
            2
        }

        fn run(self, _: &Cancel) -> Result<Self::Output, JobError> {
            assert_eq!(PHASE.get(), 1);
            Ok([7, 11])
        }
    }

    impl WireJob for Observed {
        const NAME: &'static str = "test.observed";

        fn encode(&self, _: &mut Vec<u8>) {}

        fn decode(_: &[u8]) -> Result<Self, JobError> {
            assert_eq!(PHASE.get(), 0);
            Ok(Self)
        }

        fn encode_output(output: &Self::Output, out: &mut Vec<u8>) {
            assert_eq!(PHASE.get(), 2);
            out.push(output[0]);
        }

        fn decode_output(bytes: &[u8]) -> Result<Self::Output, JobError> {
            bytes
                .try_into()
                .map_err(|_| JobError("missing output field".into()))
        }
    }

    #[test]
    fn marks_bracket_run_and_exclude_the_codecs() {
        let job = RemoteJob::of::<Observed>();
        let bytes = run_remote_job(&[job], Observed::NAME, &[], |mark| match mark {
            RunMark::Begin => assert_eq!(PHASE.replace(1), 0),
            RunMark::End => assert_eq!(PHASE.replace(2), 1),
        })
        .unwrap();
        assert_eq!(bytes, [0, 7]);
        assert_eq!(PHASE.get(), 2);
    }

    #[cfg(debug_assertions)]
    #[test]
    #[should_panic(expected = "remote job could not decode its output")]
    fn debug_roundtrip_rejects_a_missing_output_field() {
        let codec = WireCodec {
            decode: |_| Ok(Observed),
            encode_output: |output, bytes| bytes.push(output[0]),
            ..WireCodec::new()
        };
        PHASE.set(1);
        let _ = codec.roundtrip(Observed, &Cancel::default());
    }
}
