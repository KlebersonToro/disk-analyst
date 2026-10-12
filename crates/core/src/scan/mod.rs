//! Varredura de diretórios. Por ora só a enumeração de uma pasta no Linux (T015), o leitor do
//! buffer de diretório do Windows (T016, puro, compilado em todos os alvos) e o esqueleto da FFI
//! de Windows; a enumeração de Windows chega com a T017 e o varredor paralelo com a T018.

#[cfg(unix)]
#[allow(dead_code, reason = "consumido pelo varredor paralelo da T018")]
pub(crate) mod unix;
pub(crate) mod windows;
