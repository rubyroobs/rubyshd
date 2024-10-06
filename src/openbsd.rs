use log::debug;

#[cfg(target_os = "openbsd")]
use openbsd::unveil;

#[cfg(target_os = "openbsd")]
pub fn setup_unveil() {
  debug!("openbsd, calling unveil");
  unveil("/dev/urandom", "r").expect("could not unveil urandom");
  unveil(PUBLIC_PATH, "r").expect("could not unveil public data folder");
  unveil(ERRORS_PATH, "r").expect("could not unveil errors data folder");
  unveil(TLS_CLIENT_CA_CERTIFICATE_PEM_FILENAME, "r")
      .expect("could not unveil TLS CA certificate");
  unveil(TLS_SERVER_CERTIFICATE_PEM_FILENAME, "r")
      .expect("could not unveil TLS server certificate");
  unveil(TLS_SERVER_PRIVATE_KEY_PEM_FILENAME, "r")
      .expect("could not unveil TLS server private key");

  unveil::disable();
}

#[cfg(not(target_os = "openbsd"))]
pub fn setup_unveil() {
  debug!("not openbsd. :(");
}