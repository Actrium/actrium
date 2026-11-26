use actr_framework::{Context, Reply};
use actr_protocol::{ActorResult, ActrError};
use async_trait::async_trait;
use std::sync::{Arc, Mutex};

pub(super) mod upload {
    include!(concat!(env!("OUT_DIR"), "/upload.rs"));
}

pub(super) mod dispatcher {
    include!(concat!(env!("OUT_DIR"), "/upload_dispatch.rs"));
}

pub(super) struct Upload;
impl dispatcher::private::Sealed for Upload {}

#[async_trait]
impl dispatcher::ActrDispatchHandler for Upload {
    async fn upload_finish_upload<C: Context>(
        &self,
        request: upload::FinishUploadRequest,
        ctx: &C,
        reply: Reply<upload::FinishUploadResponse>,
    ) -> ActorResult<()> {
        match request.mode.as_str() {
            "expired-deadline" => {
                let deadline = reply.deadline().unwrap();
                assert!(deadline <= actr_framework::ReplyDeadline::now());
                reply.send(Ok(upload::FinishUploadResponse {
                    file_id: deadline.native_ticks().to_string(),
                }));
                return Ok(());
            }
            "inline" => {
                let file_id = reply_deadline_marker(&reply);
                reply.send(Ok(upload::FinishUploadResponse { file_id }));
                return Ok(());
            }
            "drop" => return Ok(()),
            "error" => return Err(ActrError::InvalidArgument("bad upload".into())),
            "callback" => {
                // Submit an older invocation's reply from within a newer
                // native ABI entry that has its own active token.
                let reply = Arc::new(Mutex::new(Some(reply)));
                return ctx
                    .register_stream(request.stream_id, move |chunk, _| {
                        let reply = reply.lock().unwrap().take();
                        Box::pin(async move {
                            if let Some(reply) = reply {
                                reply.send(chunk_response(chunk.payload.as_ref()));
                            }
                            Ok(())
                        })
                    })
                    .await;
            }
            _ => {}
        }

        let (sender, receiver) = tokio::sync::oneshot::channel();
        let sender = Arc::new(Mutex::new(Some(sender)));
        ctx.register_stream(request.stream_id, move |chunk, _| {
            let sender = sender.lock().unwrap().take();
            Box::pin(async move {
                if let Some(sender) = sender {
                    let _ = sender.send(chunk_response(chunk.payload.as_ref()));
                }
                Ok(())
            })
        })
        .await?;
        actr_framework::guest::dynclib::spawn(async move {
            if let Ok(result) = receiver.await {
                reply.send(result);
            }
        })
    }

    async fn upload_status<C: Context>(
        &self,
        _: upload::Empty,
        _: &C,
    ) -> ActorResult<upload::Empty> {
        Ok(upload::Empty {})
    }
}

fn reply_deadline_marker(reply: &Reply<upload::FinishUploadResponse>) -> String {
    if reply.deadline().is_some() {
        "inline-with-deadline".into()
    } else {
        "inline".into()
    }
}

fn chunk_response(payload: &[u8]) -> ActorResult<upload::FinishUploadResponse> {
    let file_id =
        std::str::from_utf8(payload).map_err(|e| ActrError::InvalidArgument(e.to_string()))?;
    Ok(upload::FinishUploadResponse {
        file_id: file_id.into(),
    })
}
