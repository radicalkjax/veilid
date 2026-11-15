// wasm-bindgen copies this snippet to pkg/snippets/<crate>-<hash>/js/, so the three
// levels up land at the package root regardless of the hash. Going through the shipped
// module rather than defining the classes here is what keeps one class identity for
// both the glue and package consumers.
import { newVeilidAPIError } from '../../../veilid_errors.js';

/// Hands the factory to Rust, which installs it as the builder for thrown errors.
export function veilidErrorFactory() {
  return newVeilidAPIError;
}
