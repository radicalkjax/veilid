use crate::*;
use std::io;

fn network_result_of(e: io::Error) -> io::Result<NetworkResult<()>> {
    Err::<(), _>(e).into_network_result()
}

pub fn test_io_error_into_network_result() {
    info!("testing path-gone os errors");
    #[cfg(unix)]
    let gone = [
        libc::EHOSTUNREACH,
        libc::ENETUNREACH,
        libc::ENETDOWN,
        libc::EHOSTDOWN,
        libc::ENETRESET,
        libc::ENXIO,
    ];
    #[cfg(windows)]
    let gone = [
        winapi::um::winsock2::WSAEHOSTUNREACH,
        winapi::um::winsock2::WSAENETUNREACH,
        winapi::um::winsock2::WSAENETDOWN,
        winapi::um::winsock2::WSAEHOSTDOWN,
        winapi::um::winsock2::WSAENETRESET,
    ];
    #[cfg(not(any(unix, windows)))]
    let gone: [i32; 0] = [];
    for code in gone {
        let res = network_result_of(io::Error::from_raw_os_error(code));
        assert!(
            matches!(res, Ok(NetworkResult::NoConnection(_))),
            "os error {code}"
        );
    }

    info!("testing error kinds");
    let res = network_result_of(io::Error::new(io::ErrorKind::BrokenPipe, "pipe"));
    assert!(matches!(res, Ok(NetworkResult::NoConnection(_))));
    let res = network_result_of(io::Error::new(io::ErrorKind::TimedOut, "slow"));
    assert!(matches!(res, Ok(NetworkResult::Timeout)));
    let res = network_result_of(io::Error::new(io::ErrorKind::InvalidData, "bad"));
    assert!(matches!(res, Ok(NetworkResult::InvalidMessage(_))));

    info!("testing other errors stay errors");
    let res = network_result_of(io::Error::new(io::ErrorKind::PermissionDenied, "denied"));
    assert!(res.is_err());
}
