// UniFFI 0.31.1 error-template excerpt for postprocessor regression coverage.
// Not a compilable SDK binding; real bindings are generated with the native library.

public enum ActrError: Swift.Error, Equatable, Hashable, Foundation.LocalizedError {



    case Unavailable(msg: String
    )
    case ConnectionNotReady(info: ConnectionNotReadyInfo
    )
    case TimedOut
    case NotFound(msg: String
    )
    case PermissionDenied(msg: String
    )
    case InvalidArgument(msg: String
    )
    case UnknownRoute(msg: String
    )
    case DependencyNotFound(serviceName: String, detail: String
    )
    case DecodeFailure(msg: String
    )
    case NotImplemented(msg: String
    )
    case Internal(msg: String
    )
    /**
     * Config file parsing / trust resolution failed before the runtime could
     * hand the request over to the protocol layer.
     *
     * Classified as `ErrorKind::Client` — the caller supplied a bad manifest
     * or runtime config.
     */
    case Config(msg: String
    )

    case DeliveryUncertain(operationId: String, cause: String
    )






    public var errorDescription: String? {
        actrErrorMessage(err: self)
    }

}

#if compiler(>=6)
extension ActrError: Sendable {}
#endif

public func actrErrorMessage(err: ActrError) -> String {
    return try! FfiConverterString.lift(try! rustCall() {
        uniffi_actr_fn_func_actr_error_message(FfiConverterTypeActrError_lower(err), $0)
    })
}
