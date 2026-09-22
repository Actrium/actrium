import Foundation

public extension ActrType {
    /// Returns a string representation of the actor type in the format "publisher/program@version".
    ///
    /// Example: `ActrType(publisher: "acme", name: "EchoService", version: "1.0.0").toStringRepr()` returns `"acme/EchoService@1.0.0"`
    func toStringRepr() -> String {
        guard !version.isEmpty else {
            fatalError("ActrType.version must be non-empty")
        }
        return "\(publisher)/\(name)@\(version)"
    }

    /// Creates an `ActrType` from a string representation in the format "publisher/program@version".
    ///
    /// - Parameter stringRepr: String representation in the format "publisher/program@version" (e.g., "acme/EchoService@1.0.0")
    /// - Returns: An `ActrType` instance
    /// - Throws: `ActrError.Config` if the string format is invalid or contains invalid characters
    ///
    /// Example:
    /// ```swift
    /// let type = try ActrType.fromStringRepr("acme/EchoService@1.0.0")
    /// // type.publisher == "acme"
    /// // type.name == "EchoService"
    /// // type.version == "1.0.0"
    /// ```
    static func fromStringRepr(_ stringRepr: String) throws -> ActrType {
        let expected = "Invalid ActrType format: '\(stringRepr)'. Expected format: publisher/program@version (e.g., acme/EchoService@1.0.0)"
        guard let slash = stringRepr.firstIndex(of: "/"),
              let at = stringRepr.lastIndex(of: "@"),
              slash < at
        else {
            throw ActrError.Config(msg: expected)
        }

        let publisher = String(stringRepr[..<slash])
        let name = String(stringRepr[stringRepr.index(after: slash)..<at])
        let version = String(stringRepr[stringRepr.index(after: at)...])
        guard !name.contains("/") else {
            throw ActrError.Config(msg: expected)
        }

        // Validate that publisher and name are not empty
        guard !publisher.isEmpty else {
            throw ActrError.Config(msg: "Invalid publisher: publisher cannot be empty")
        }

        guard !name.isEmpty else {
            throw ActrError.Config(msg: "Invalid type name: name cannot be empty")
        }

        guard !version.isEmpty else {
            throw ActrError.Config(msg: "Invalid version: version cannot be empty")
        }

        // Basic validation: publisher and name should not contain invalid characters
        // This is a simplified validation. For stricter validation matching Rust's Name validation,
        // you may need to add more checks based on the Name validation rules.
        let invalidChars = CharacterSet(charactersIn: "+@:/")
        if publisher.rangeOfCharacter(from: invalidChars) != nil {
            throw ActrError.Config(msg: "Invalid publisher: '\(publisher)' contains invalid characters")
        }

        if name.rangeOfCharacter(from: invalidChars) != nil {
            throw ActrError.Config(msg: "Invalid type name: '\(name)' contains invalid characters")
        }

        return ActrType(publisher: publisher, name: name, version: version)
    }
}
