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
 * identity set. actor_type must be <manufacturer>:<name>:<version>. */
actr_node_init_handle actr_node_with_actor_type(actr_node_init_handle init_handle,
                                                const char* actor_type_ptr,
                                                size_t actor_type_len);
void* actr_workload_create(const char* name_ptr, size_t name_len);
int32_t actr_node_link(void* init_handle, void* workload_handle,
                       actr_handle_callback callback, void* user_data);
void* actr_node_with_observer(void* attached_handle, actr_observer_callbacks callbacks);
int32_t actr_node_register(void* attached_handle,
                           const char* ais_ptr, size_t ais_len,
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

/* Memory management */
void actr_free_bytes(uint8_t* ptr, size_t len);
void actr_free_string(char* ptr, size_t len);
void actr_release_handle(void* handle);
void actr_release_actr_ref(void* handle);

#ifdef __cplusplus
}
#endif

#endif /* ACTR_CABI_H */
