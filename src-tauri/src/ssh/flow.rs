use std::future::Future;
use std::sync::atomic::{AtomicU64, Ordering};
use std::task::Poll;

use tokio::sync::{Notify, watch};

pub(crate) const OUTPUT_WINDOW: u64 = 1024 * 1024;

pub(crate) struct OutputWindow {
    unacked: AtomicU64,
    notify: Notify,
}

impl OutputWindow {
    pub(crate) fn new() -> Self {
        Self {
            unacked: AtomicU64::new(0),
            notify: Notify::new(),
        }
    }

    pub(crate) fn charge(&self, bytes: u64) {
        if bytes == 0 {
            return;
        }
        let _ = self.unacked.try_update(Ordering::Release, Ordering::Relaxed, |current| {
            Some(current.saturating_add(bytes))
        });
    }

    pub(crate) fn release(&self, bytes: u64) {
        if bytes == 0 {
            return;
        }
        let updated = self.unacked.try_update(Ordering::AcqRel, Ordering::Acquire, |current| {
            let next = current.saturating_sub(bytes);
            (next != current).then_some(next)
        });
        if updated.is_ok() {
            self.notify.notify_waiters();
        }
    }

    pub(crate) fn blocked(&self) -> bool {
        self.unacked.load(Ordering::Acquire) >= OUTPUT_WINDOW
    }
}

pub(crate) async fn wait_for_output_room(window: &OutputWindow, cancel: &mut watch::Receiver<bool>) -> bool {
    loop {
        if *cancel.borrow() {
            return false;
        }
        if !window.blocked() {
            return true;
        }
        let notified = window.notify.notified();
        tokio::pin!(notified);
        let woke = std::future::poll_fn(|cx| {
            Poll::Ready(matches!(notified.as_mut().poll(cx), Poll::Ready(())))
        })
        .await;
        if *cancel.borrow() {
            return false;
        }
        if !window.blocked() {
            return true;
        }
        if woke {
            continue;
        }
        tokio::select! {
            _ = &mut notified => {}
            _ = cancel.wait_for(|cancelled| *cancelled) => return false,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use super::{OUTPUT_WINDOW, OutputWindow, wait_for_output_room};
    use tokio::sync::watch;

    #[test]
    fn extra_acks_do_not_open_the_window_past_zero() {
        let window = OutputWindow::new();
        window.charge(10);
        window.release(50);
        window.release(0);
        assert!(!window.blocked());
        window.charge(OUTPUT_WINDOW);
        assert!(window.blocked());
        window.release(1);
        assert!(!window.blocked());
    }

    #[tokio::test]
    async fn a_full_window_waits_until_the_terminal_acks() {
        let window = Arc::new(OutputWindow::new());
        window.charge(OUTPUT_WINDOW);
        let (_cancel_tx, mut cancel) = watch::channel(false);
        let waiting = {
            let window = Arc::clone(&window);
            tokio::spawn(async move { wait_for_output_room(&window, &mut cancel).await })
        };
        tokio::time::sleep(Duration::from_millis(40)).await;
        assert!(!waiting.is_finished());
        window.release(1);
        let opened = tokio::time::timeout(Duration::from_secs(1), waiting)
            .await
            .expect("ack should wake the reader")
            .expect("waiter task");
        assert!(opened);
    }

    #[tokio::test]
    async fn an_ack_that_arrives_before_the_wait_is_not_missed() {
        let window = OutputWindow::new();
        window.charge(OUTPUT_WINDOW);
        window.release(OUTPUT_WINDOW);
        let (_cancel_tx, mut cancel) = watch::channel(false);
        let opened = tokio::time::timeout(
            Duration::from_secs(1),
            wait_for_output_room(&window, &mut cancel),
        )
        .await
        .expect("room was already available");
        assert!(opened);
    }

    #[tokio::test]
    async fn disconnect_unblocks_a_full_window() {
        let window = OutputWindow::new();
        window.charge(OUTPUT_WINDOW);
        let (cancel_tx, mut cancel) = watch::channel(false);
        let waiting = tokio::spawn(async move { wait_for_output_room(&window, &mut cancel).await });
        tokio::time::sleep(Duration::from_millis(20)).await;
        cancel_tx.send(true).expect("cancel sender");
        let opened = tokio::time::timeout(Duration::from_secs(1), waiting)
            .await
            .expect("disconnect should stop the wait")
            .expect("waiter task");
        assert!(!opened);
    }
}
