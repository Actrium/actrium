#ifndef ACTR_CABI_H
#define ACTR_CABI_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* Opaque handles are non-zero tokens owned by actr-cabi. Never dereference
 * them; release them only through the matching actr-cabi function. */
typedef void* actr_node_init_handle;
typedef void* actr_node_attached_handle;
typedef void* actr_node_registered_handle;
typedef void* actr_ref_handle;
typedef void* actr_workload_handle;

/* Completion callbacks. Every buffer pointer passed to a callback
 * (error_ptr, resp_ptr, actor_id_ptr) is valid ONLY for the duration of the
 * callback; the consumer must copy before returning. None of these buffers
 * are owned by the caller — do not call actr_free_bytes/actr_free_string on
 * them. A null error_ptr with error_len 0 means no error. */
typedef void (*actr_void_callback)(int32_t status,
                                  const char* error_ptr, size_t error_len,
                                  void* user_data);
typedef void (*actr_handle_callback)(int32_t status,
                                     void* result_handle,
                                     const char* error_ptr, size_t error_len,
                                     void* user_data);
typedef void (*actr_call_callback)(int32_t status,
                                   const uint8_t* resp_ptr, size_t resp_len,
                                   const char* error_ptr, size_t error_len,
                                   void* user_data);
typedef void (*actr_discover_callback)(int32_t status,
                                       const char* actor_id_ptr, size_t actor_id_len,
                                       const char* error_ptr, size_t error_len,
                                       void* user_data);

/* Observer callback table. user_data is passed verbatim to every callback.
 * The on_error msg_ptr is valid ONLY for the duration of the callback; copy
 * before returning. */
typedef struct {
    void* user_data;
    void (*on_signaling_connecting)(void* user_data);
    void (*on_signaling_connected)(void* user_data);
    void (*on_signaling_disconnected)(void* user_data);
    void (*on_error)(const uint8_t* msg_ptr, size_t msg_len, void* user_data);
} actr_observer_callbacks;

/* Borrowed metadata carried by a forwarded DataChunk. The entry and the key
 * and value buffers are valid ONLY for the stream sink callback duration. */
typedef struct {
    const char* key_ptr;
    size_t key_len;
    const char* value_ptr;
    size_t value_len;
} actr_stream_metadata_entry;

/* Host bridge — stream sink. Fired when a DataChunk forwarded by
 * actr_stream_listen arrives. chunk_flags bit 0 (0x01) indicates timestamp_ms
 * is set; other bits are reserved and must be ignored. All pointers are
 * valid ONLY for the duration of the callback; copy before returning.
 * metadata_ptr is an array of metadata_len borrowed actr_stream_metadata_entry
 * values (or NULL when metadata_len is zero).
 * sender_id is the sending actor's string repr. Callbacks run synchronously
 * on actr-cabi runtime worker threads, may re-enter actr-cabi except to
 * replace/clear a sink, and must not unwind across the C ABI boundary. */
typedef void (*actr_stream_sink_callback)(const char* stream_id_ptr, size_t stream_id_len,
                                          const uint8_t* payload_ptr, size_t payload_len,
                                          const actr_stream_metadata_entry* metadata_ptr,
                                          size_t metadata_len,
                                          uint64_t sequence,
                                          uint8_t chunk_flags, int64_t timestamp_ms,
                                          const char* sender_id_ptr, size_t sender_id_len,
                                          void* user_data);

/* Host bridge — reverse RPC. The host receives request_id + route + payload
 * and must eventually answer request_id via actr_host_respond_reverse_rpc
 * (an unanswered request times out on the Rust side). Same buffer validity
 * and threading contract as actr_stream_sink_callback. */
typedef void (*actr_reverse_rpc_callback)(uint64_t request_id,
                                          const char* route_ptr, size_t route_len,
                                          const uint8_t* payload_ptr, size_t payload_len,
                                          void* user_data);

/* Runtime */
int32_t actr_init_runtime(void);
int32_t actr_shutdown_runtime(void);

/* Native introspection — results point at static memory; do NOT free.
 * A host calls these before driving the typestate chain to confirm the
 * loaded actr_cabi matches the managed binding (ABI + actr version) and to
 * surface which workload bundle was linked. out_len receives the byte length
 * and may be NULL. String results are not NUL-terminated; callers must read
 * exactly out_len bytes instead of using strlen or %s. */
uint32_t actr_cabi_abi_version(void);
const char* actr_cabi_actr_version(size_t* out_len);
const char* actr_cabi_bundle_id(size_t* out_len);
uint32_t actr_cabi_bundle_count(void);

/* Node typestate chain */
int32_t actr_node_from_config(const char* path_ptr, size_t path_len,
                              actr_handle_callback callback, void* user_data);
/* Consume an init handle and return a replacement with the linked actor
 * identity set. actor_type must be <publisher>:<name>:<version>. */
actr_node_init_handle actr_node_with_actor_type(actr_node_init_handle init_handle,
                                                const char* actor_type_ptr,
                                                size_t actor_type_len);
void* actr_workload_create(const char* name_ptr, size_t name_len);
int32_t actr_node_link(void* init_handle, void* workload_handle,
                       actr_handle_callback callback, void* user_data);
void* actr_node_with_observer(void* attached_handle, actr_observer_callbacks callbacks);
int32_t actr_node_register(void* attached_handle,
                           const char* issuer_ptr, size_t issuer_len,
                           actr_handle_callback callback, void* user_data);
int32_t actr_node_start(void* registered_handle,
                       actr_handle_callback callback, void* user_data);

/* ActrRef */
int32_t actr_ref_call(void* ref_handle,
                      const char* route_key_ptr, size_t route_key_len,
                      const uint8_t* payload_ptr, size_t payload_len,
                      int64_t timeout_ms,
                      actr_call_callback callback, void* user_data);
int32_t actr_ref_discover(void* ref_handle,
                          const char* type_ptr, size_t type_len,
                          actr_discover_callback callback, void* user_data);
int32_t actr_ref_shutdown(void* ref_handle);
int32_t actr_ref_wait_shutdown(void* ref_handle,
                               actr_void_callback callback, void* user_data);

/* Host bridge: stream sink + reverse RPC. Registering replaces any previous
 * sink; passing NULL clears it. Sinks are process-global (single actr_cabi
 * provider per process). Clear/replace waits for in-flight callbacks before
 * returning, so old user_data may be released then. Calling either register
 * function from a sink callback returns -50 rather than deadlocking. Chunks
 * delivered while no stream sink is registered are dropped, not buffered. */
int32_t actr_register_stream_sink(actr_stream_sink_callback sink, void* user_data);
int32_t actr_register_reverse_rpc_sink(actr_reverse_rpc_callback sink, void* user_data);

/* Deliver the host's answer for a reverse-RPC request. status == 0 resolves
 * the awaiting ask_host caller with payload; any other status resolves it
 * with an error. Returns -22 (REQUEST_ID_UNKNOWN) when request_id is
 * unknown, already answered, or past its timeout. payload_ptr may be NULL
 * with payload_len == 0 for an empty payload. Synchronous; safe from any
 * thread, including inside the reverse-RPC sink callback. */
int32_t actr_host_respond_reverse_rpc(uint64_t request_id, int32_t status,
                                      const uint8_t* payload_ptr, size_t payload_len);

/* Subscribe the node to exactly one inbound data stream id and forward every
 * matching chunk to the registered stream sink. Async — registration result via callback.
 * One listener per stream_id: a second listen for the same id follows the
 * runtime's register_stream semantics. Unlisten is idempotent; it removes
 * that exact listener and reports completion through the callback. Chunks
 * already accepted by the runtime may still finish their callback. */
int32_t actr_stream_listen(void* ref_handle,
                           const char* stream_id_ptr, size_t stream_id_len,
                           actr_void_callback callback, void* user_data);
int32_t actr_stream_unlisten(void* ref_handle,
                             const char* stream_id_ptr, size_t stream_id_len,
                             actr_void_callback callback, void* user_data);

/* Memory management */
void actr_free_bytes(uint8_t* ptr, size_t len);
void actr_free_string(char* ptr, size_t len);
void actr_release_handle(void* handle);
void actr_release_actr_ref(void* handle);

#ifdef __cplusplus
}
#endif

#endif /* ACTR_CABI_H */
