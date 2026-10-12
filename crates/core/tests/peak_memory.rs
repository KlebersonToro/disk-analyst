//! O pico de memória do processo cresce com o tempo, então medir depois de tocar 64 MiB dá no
//! mínimo 64 MiB, mesmo que outros testes do mesmo processo rodem em paralelo.

use std::hint::black_box;

use disk_analyst_core::sysinfo::peak_memory_bytes;

const MIB64: usize = 64 * 1024 * 1024;

#[test]
fn peak_sees_a_touched_64_mib_allocation() {
    let mut buffer = vec![0u8; MIB64];
    // Uma escrita por página de 4 KiB: as páginas ficam residentes de fato.
    for i in (0..MIB64).step_by(4096) {
        buffer[i] = 1;
    }
    black_box(&buffer);

    let peak = peak_memory_bytes().expect("pico de memória");
    println!("pico medido: {peak} bytes");
    assert!(peak >= MIB64 as u64, "pico {peak} abaixo de 64 MiB");
}
