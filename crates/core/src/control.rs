//! Controle de uma varredura: cancelamento (ligado pela CLI no Ctrl+C) e contadores de progresso.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

/// Sinal de cancelamento compartilhado. Clones apontam para o mesmo estado.
///
/// `Relaxed` basta: é só um sinal booleano e não publica outros dados. Quem lê não precisa ver
/// escritas anteriores ao `cancel`; ele só precisa, mais cedo ou mais tarde, enxergar o `true`.
#[derive(Clone, Debug, Default)]
pub struct CancelToken(Arc<AtomicBool>);

impl CancelToken {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

/// Contadores de progresso, atualizados pelas threads da varredura e lidos pela CLI.
///
/// Todos usam `Relaxed`: são só números para exibir, sem ordem entre eles.
#[derive(Debug, Default)]
pub struct ScanCounters {
    items: AtomicU64,
    files: AtomicU64,
    bytes: AtomicU64,
}

/// Cópia dos contadores num instante.
///
/// Não é atômica entre os três campos: cada um é lido separadamente. Serve para exibir progresso,
/// não para contas que precisem de consistência entre campos.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CountersSnapshot {
    pub items: u64,
    pub files: u64,
    pub bytes: u64,
}

impl ScanCounters {
    pub fn add_items(&self, n: u64) {
        self.items.fetch_add(n, Ordering::Relaxed);
    }

    pub fn add_files(&self, n: u64) {
        self.files.fetch_add(n, Ordering::Relaxed);
    }

    pub fn add_bytes(&self, n: u64) {
        self.bytes.fetch_add(n, Ordering::Relaxed);
    }

    pub fn snapshot(&self) -> CountersSnapshot {
        CountersSnapshot {
            items: self.items.load(Ordering::Relaxed),
            files: self.files.load(Ordering::Relaxed),
            bytes: self.bytes.load(Ordering::Relaxed),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_token_is_not_cancelled() {
        assert!(!CancelToken::new().is_cancelled());
    }

    #[test]
    fn clone_shares_cancellation_in_both_directions() {
        let original = CancelToken::new();
        let clone = original.clone();

        clone.cancel();
        assert!(original.is_cancelled());

        let other = CancelToken::new();
        let other_clone = other.clone();
        other.cancel();
        assert!(other_clone.is_cancelled());
    }

    #[test]
    fn clone_moved_to_another_thread_cancels_the_original() {
        let token = CancelToken::new();
        let worker = token.clone();
        std::thread::spawn(move || worker.cancel())
            .join()
            .expect("a thread de cancelamento não deve panicar");
        assert!(token.is_cancelled());
    }

    #[test]
    fn counters_start_at_zero() {
        let counters = ScanCounters::default();
        assert_eq!(
            counters.snapshot(),
            CountersSnapshot {
                items: 0,
                files: 0,
                bytes: 0
            }
        );
    }

    #[test]
    fn eight_threads_add_up_exactly() {
        const THREADS: u64 = 8;
        const N: u64 = 10_000;
        let counters = ScanCounters::default();

        std::thread::scope(|scope| {
            for t in 0..THREADS {
                let counters = &counters;
                scope.spawn(move || {
                    for i in 0..N {
                        counters.add_items(1);
                        counters.add_files(1);
                        counters.add_bytes(t * 1000 + i % 7);
                    }
                });
            }
        });

        let expected_bytes: u64 = (0..THREADS)
            .flat_map(|t| (0..N).map(move |i| t * 1000 + i % 7))
            .sum();
        assert_eq!(
            counters.snapshot(),
            CountersSnapshot {
                items: THREADS * N,
                files: THREADS * N,
                bytes: expected_bytes,
            }
        );
    }
}
