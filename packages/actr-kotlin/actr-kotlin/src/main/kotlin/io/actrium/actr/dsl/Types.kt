/** Type builders and extensions for Actrium types. */
package io.actrium.actr.dsl

import io.actrium.actr.ActrId
import io.actrium.actr.ActrType
import io.actrium.actr.DataChunk
import io.actrium.actr.MetadataEntry

// ============================================================================
// ActrType Builders
// ============================================================================

/**
 * Create an ActrType from a string in "publisher/program@version" format.
 *
 * @param typeString Type string like "acme/EchoService@1.0.0"
 * @return ActrType with parsed publisher, name, and version
 * @throws IllegalArgumentException if format is invalid
 */
fun String.toActrType(): ActrType {
    val slash = indexOf('/')
    val at = lastIndexOf('@')
    require(slash > 0 && at > slash + 1 && indexOf('/', slash + 1) == -1) {
        "ActrType string must be in 'publisher/program@version' format, got: $this"
    }
    val version = substring(at + 1)
    require(version.isNotBlank()) { "ActrType version must not be blank" }
    return ActrType(publisher = substring(0, slash), name = substring(slash + 1, at), version = version)
}

/**
 * Create an ActrType with DSL syntax.
 *
 * Example:
 * ```kotlin
 * val type = actrType {
 *     publisher = "acme"
 *     name = "EchoService"
 *     version = "1.0.0"
 * }
 * ```
 */
inline fun actrType(builder: ActrTypeBuilder.() -> Unit): ActrType = ActrTypeBuilder().apply(builder).build()

/**
 * Create an ActrType from publisher, name, and version.
 *
 * Example:
 * ```kotlin
 * val type = actrType("acme", "EchoService", "1.0.0")
 * ```
 */
fun actrType(
    publisher: String,
    name: String,
    version: String,
): ActrType = ActrType(publisher = publisher, name = name, version = version)

/** Builder for ActrType. */
class ActrTypeBuilder {
    var publisher: String = ""
    var name: String = ""
    var version: String = ""

    fun build(): ActrType {
        require(publisher.isNotBlank()) { "publisher must not be blank" }
        require(name.isNotBlank()) { "name must not be blank" }
        require(version.isNotBlank()) { "version must not be blank" }
        return ActrType(publisher = publisher, name = name, version = version)
    }
}

// ============================================================================
// ActrType Extensions
// ============================================================================

/** Convert ActrType to string representation. */
fun ActrType.toTypeString(): String = "$publisher/$name@$version"

/** Check if this type matches a type string. */
fun ActrType.matches(typeString: String): Boolean {
    val other = typeString.toActrType()
    return publisher == other.publisher && name == other.name
}

// ============================================================================
// ActrId Builders
// ============================================================================

/**
 * Create an ActrId with DSL syntax.
 *
 * Example:
 * ```kotlin
 * val id = actrId {
 *     realm = "k7m2q9xh@acme.com"
 *     serialNumber = 0x2af01uL
 * }
 * ```
 */
inline fun actrId(builder: ActrIdBuilder.() -> Unit): ActrId = ActrIdBuilder().apply(builder).build()

/** Builder for ActrId. */
class ActrIdBuilder {
    /** Realm name, `<realm_id>@<domain>`. */
    var realm: String = ""
    var serialNumber: ULong = 0uL

    fun build(): ActrId {
        require(realm.isNotBlank()) { "realm must be set" }
        return ActrId(realm = realm, serialNumber = serialNumber)
    }
}

// ============================================================================
// ActrId Extensions
// ============================================================================

/** RealmId part of the Realm name. */
val ActrId.realmId: String
    get() = realm.substringBefore('@', missingDelimiterValue = "")

/** Canonical form `<serial_hex>.<realm_id>@<domain>`. */
fun ActrId.toFullString(): String = "${serialNumber.toString(16)}.$realm"

// ============================================================================
// DataChunk Builders
// ============================================================================

/**
 * Create a DataChunk with DSL syntax.
 *
 * Example:
 * ```kotlin
 * val chunk = dataChunk {
 *     streamId = "file-transfer-001"
 *     sequence = 0uL
 *     payload = fileBytes
 *     timestamp = System.currentTimeMillis()
 *     metadata {
 *         "content-type" to "application/octet-stream"
 *         "filename" to "example.txt"
 *     }
 * }
 * ```
 */
inline fun dataChunk(builder: DataChunkBuilder.() -> Unit): DataChunk = DataChunkBuilder().apply(builder).build()

/** Builder for DataChunk. */
class DataChunkBuilder {
    var streamId: String = ""
    var sequence: ULong = 0uL
    var payload: ByteArray = ByteArray(0)
    var timestamp: Long? = null
    private val metadataEntries = mutableListOf<MetadataEntry>()

    /**
     * Add metadata entries using DSL.
     *
     * Example:
     * ```kotlin
     * metadata {
     *     "key1" to "value1"
     *     "key2" to "value2"
     * }
     * ```
     */
    fun metadata(builder: MetadataBuilder.() -> Unit) {
        metadataEntries.addAll(MetadataBuilder().apply(builder).entries)
    }

    /** Add a single metadata entry. */
    fun addMetadata(
        key: String,
        value: String,
    ) {
        metadataEntries.add(MetadataEntry(key = key, value = value))
    }

    /** Set payload from a string. */
    fun payload(
        text: String,
        charset: java.nio.charset.Charset = Charsets.UTF_8,
    ) {
        payload = text.toByteArray(charset)
    }

    fun build(): DataChunk {
        require(streamId.isNotBlank()) { "streamId must not be blank" }
        return DataChunk(
            streamId = streamId,
            sequence = sequence,
            payload = payload,
            metadata = metadataEntries.toList(),
            timestampMs = timestamp,
        )
    }
}

/** Builder for metadata entries. */
class MetadataBuilder {
    internal val entries = mutableListOf<MetadataEntry>()

    /** Add a metadata entry using infix notation. */
    infix fun String.to(value: String) {
        entries.add(MetadataEntry(key = this, value = value))
    }
}

// ============================================================================
// DataChunk Extensions
// ============================================================================

/** Get a metadata value by key. */
fun DataChunk.getMetadata(key: String): String? = metadata.find { it.key == key }?.value

/** Check if metadata contains a key. */
fun DataChunk.hasMetadata(key: String): Boolean = metadata.any { it.key == key }

/** Get metadata as a Map. */
fun DataChunk.metadataMap(): Map<String, String> = metadata.associate { it.key to it.value }
