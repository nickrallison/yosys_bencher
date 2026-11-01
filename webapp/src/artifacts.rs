use yosys_bench_ui_types::store::ArtifactId;

pub fn url(id: ArtifactId) -> String {
    format!("/artifacts/{id}")
}

#[cfg(feature = "server")]
mod server {
    use super::*;
    use crate::api::{ArtifactData, backend};
    use dioxus::server::axum::{
        extract::{Path, Query},
        http::{StatusCode, header},
        response::{IntoResponse, Response},
    };
    use serde::Deserialize;
    use yosys_bench_ui_types::api::ErrorKind;

    #[derive(Default, Deserialize)]
    pub struct Options {
        #[serde(default)]
        download: bool,
    }

    pub async fn serve(Path(id): Path<i64>, Query(options): Query<Options>) -> Response {
        match backend(|b| {
            let artifact = b.store.artifact(ArtifactId(id))?;
            if !artifact.available {
                return Err(yosys_bench::store::StoreError::NotFound(format!(
                    "artifact {id} is unavailable"
                )));
            }
            Ok(ArtifactData {
                name: artifact.name,
                bytes: b.read_artifact(ArtifactId(id))?,
            })
        }) {
            Ok(file) => response(file, options.download),
            Err(error) => (
                if error.kind == ErrorKind::NotFound {
                    StatusCode::NOT_FOUND
                } else {
                    StatusCode::INTERNAL_SERVER_ERROR
                },
                error.message,
            )
                .into_response(),
        }
    }

    fn response(file: ArtifactData, download: bool) -> Response {
        let content_type = if file.name.ends_with(".svg") {
            "image/svg+xml"
        } else if !file.bytes.contains(&0) && std::str::from_utf8(&file.bytes).is_ok() {
            "text/plain; charset=utf-8"
        } else {
            "application/octet-stream"
        };
        let filename = file.name.rsplit('/').next().unwrap_or("artifact");
        let encoded: String = filename
            .bytes()
            .map(|byte| {
                if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) {
                    char::from(byte).to_string()
                } else {
                    format!("%{byte:02X}")
                }
            })
            .collect();
        let disposition = if download { "attachment" } else { "inline" };
        (
            [
                (header::CONTENT_TYPE, content_type.to_string()),
                (
                    header::CONTENT_DISPOSITION,
                    format!("{disposition}; filename*=UTF-8''{encoded}"),
                ),
                (header::CACHE_CONTROL, "no-store".into()),
                (header::X_CONTENT_TYPE_OPTIONS, "nosniff".into()),
                // FlameGraph uses inline scripts for search and zoom.
                (
                    header::CONTENT_SECURITY_POLICY,
                    "default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'"
                        .into(),
                ),
            ],
            file.bytes,
        )
            .into_response()
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use dioxus::server::axum::body::HttpBody;

        #[test]
        fn svg_opens_inline_and_downloads_keep_the_filename() {
            let file = || ArtifactData {
                name: "perf/optimized/flame graph.svg".into(),
                bytes: b"<svg xmlns=\"http://www.w3.org/2000/svg\"/>".to_vec(),
            };
            let inline = response(file(), false);
            assert_eq!(inline.status(), StatusCode::OK);
            assert_eq!(inline.headers()[header::CONTENT_TYPE], "image/svg+xml");
            assert_eq!(
                inline.headers()[header::CONTENT_DISPOSITION],
                "inline; filename*=UTF-8''flame%20graph.svg"
            );
            assert!(
                inline.headers()[header::CONTENT_SECURITY_POLICY]
                    .to_str()
                    .unwrap()
                    .contains("script-src 'unsafe-inline'")
            );
            assert_eq!(
                response(file(), true).headers()[header::CONTENT_DISPOSITION],
                "attachment; filename*=UTF-8''flame%20graph.svg"
            );
        }

        #[test]
        fn logs_are_plain_text_and_recordings_are_binary() {
            for (name, bytes, expected) in [
                (
                    "run.ys",
                    b"read_verilog design.v".as_slice(),
                    "text/plain; charset=utf-8",
                ),
                (
                    "perf.data",
                    b"PERFILE2\0".as_slice(),
                    "application/octet-stream",
                ),
                (
                    "output.html",
                    b"<script>alert(1)</script>".as_slice(),
                    "text/plain; charset=utf-8",
                ),
            ] {
                let reply = response(
                    ArtifactData {
                        name: name.into(),
                        bytes: bytes.to_vec(),
                    },
                    false,
                );
                assert_eq!(reply.headers()[header::CONTENT_TYPE], expected);
                assert_eq!(reply.body().size_hint().exact(), Some(bytes.len() as u64));
            }
        }
    }
}

#[cfg(feature = "server")]
pub use server::serve;
