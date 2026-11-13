//! Fire-and-forget automation run monitoring and Windows stdio detach; run natively on Windows CI (DEV-495).

mod async_run_monitor_tests {
    use lotar::services::automation_service::{BoundedTail, stderr_eof_within_grace};
    use std::sync::mpsc;
    use std::time::{Duration, Instant};

    #[test]
    fn bounded_tail_keeps_final_bytes_and_reports_dropped_prefix() {
        let mut tail = BoundedTail::new(8);
        tail.push(b"abcdefgh");
        assert_eq!(tail.dropped, 0);
        assert_eq!(tail.data, b"abcdefgh".to_vec());

        tail.push(b"ijklmn");
        assert_eq!(tail.data, b"ghijklmn".to_vec());
        assert_eq!(tail.dropped, 6);

        let snapshot = tail.snapshot();
        assert!(snapshot.contains("[+6 earlier stderr bytes]\n"), "{snapshot}");
        assert!(snapshot.ends_with("ghijklmn"), "{snapshot}");
    }

    #[test]
    fn bounded_tail_zero_limit_drops_everything() {
        let mut tail = BoundedTail::new(0);
        tail.push(b"xyz");
        assert_eq!(tail.dropped, 3);
        assert!(tail.data.is_empty());
        assert_eq!(tail.snapshot(), "");
    }

    #[test]
    fn eof_grace_treats_closed_channel_as_drained_and_silence_as_held() {
        // A live-but-silent pipe (descendant holds the write end) only
        // waits out the grace period before being reported as held.
        let (silent_tx, silent_rx) = mpsc::channel::<()>();
        let start = Instant::now();
        assert!(!stderr_eof_within_grace(&silent_rx, Duration::from_millis(50)));
        assert!(start.elapsed() >= Duration::from_millis(45), "grace returned early: {:?}", start.elapsed());

        // A reader that finished (message sent, then channel closed) is
        // drained, including the already-disconnected case.
        drop(silent_tx);
        assert!(stderr_eof_within_grace(&silent_rx, Duration::from_millis(1)));

        let (tx, rx) = mpsc::channel::<()>();
        tx.send(()).unwrap();
        assert!(stderr_eof_within_grace(&rx, Duration::from_millis(1)));
    }
}

/// Native Windows proof of the stdio-inheritance scope. Runs in the Windows
/// CI selection; the end-to-end coupling regression lives in
/// the portable smoke selection (`cli.automation` /
/// `api.automation.smoke.spec.ts`).
#[cfg(windows)]
mod windows_async_stdio_tests {
    use lotar::services::automation_service::windows_async_stdio as sys;

    const FILE_TYPE_PIPE: u32 = 0x0003;

    #[test]
    fn detach_clears_inherit_bit_on_pipe_stdio() {
        for (name, slot) in [("stdout", sys::STD_OUTPUT_HANDLE), ("stderr", sys::STD_ERROR_HANDLE)] {
            let file_type = sys::std_slot_file_type(slot);
            let outcome = sys::clear_std_slot(slot);
            let flags = sys::std_slot_flags(slot);
            if file_type == FILE_TYPE_PIPE {
                // Captured pipe stdio (nextest/CI): the detach must
                // verifiably clear the bit. A refusal on a real pipe is a
                // genuine failure and must not be silently accepted.
                assert_eq!(outcome, sys::SlotDetach::Cleared, "{name} std slot is a pipe; detach refused (file_type={file_type:#x})");
                assert_eq!(flags.unwrap_or(u32::MAX) & sys::HANDLE_FLAG_INHERIT, 0, "{name} pipe std slot still inheritable after detach");
            } else {
                // Console/character handles legitimately refuse
                // SetHandleInformation; interactive runs take this arm and
                // only report/query consistency is enforced there.
                match (outcome, flags) {
                    (sys::SlotDetach::NoHandle, None) => {}
                    (sys::SlotDetach::NotClearable, _) => {}
                    (sys::SlotDetach::Cleared, Some(flags)) => {
                        assert_eq!(flags & sys::HANDLE_FLAG_INHERIT, 0, "{name} std slot still inheritable after detach");
                    }
                    other => panic!("{name}: inconsistent detach state {other:?} (file_type={file_type:#x})"),
                }
            }
        }
    }

    #[test]
    fn detach_is_idempotent_across_calls() {
        sys::detach_stdio_for_async_children();
        let first = sys::clear_std_slot(sys::STD_OUTPUT_HANDLE);
        let second = sys::clear_std_slot(sys::STD_OUTPUT_HANDLE);
        assert_eq!(first, second);
        // Clearing an already-cleared slot stays cleared (idempotent
        // outcome, not a flip-flop of the inherit bit).
        if let sys::SlotDetach::Cleared = second {
            let flags = sys::std_slot_flags(sys::STD_OUTPUT_HANDLE);
            assert_eq!(flags.unwrap_or(0) & sys::HANDLE_FLAG_INHERIT, 0, "repeat detach re-armed the inherit flag");
        }
    }
}
