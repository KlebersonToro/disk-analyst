//! Varredura de diretórios. Por ora só a enumeração de uma pasta no Linux (T015) e o esqueleto da
//! parte de Windows (FFI); a enumeração de Windows chega com T017 e o varredor paralelo com T018.

#[cfg(unix)]
#[allow(dead_code, reason = "consumido pelo varredor paralelo da T018")]
pub(crate) mod unix;
#[cfg(windows)]
pub(crate) mod windows;
