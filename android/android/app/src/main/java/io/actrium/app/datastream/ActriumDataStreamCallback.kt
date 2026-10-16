package io.actrium.app.datastream

import io.actor_rtc.actr.ActrId
import io.actor_rtc.actr.DataStream
import io.actor_rtc.actr.DataStreamCallback
import com.facebook.react.bridge.Arguments
import io.actrium.app.core.ActrBridge
import io.actrium.app.rust.ActrBridgeContractInfo
import io.actrium.app.rust.ActriumCore
import android.util.Base64
import java.nio.charset.StandardCharsets

enum class StreamType {
    TEXT,
    VOICE;

    fun wireValue(contract: ActrBridgeContractInfo): String {
        return when (this) {
            TEXT -> contract.streamTypeText
            VOICE -> contract.streamTypeVoice
        }
    }
}

class ActriumDataStreamCallback(val streamId:String, val streamType: StreamType): DataStreamCallback{
    private val contract = ActriumCore.actrBridgeContract()

    override suspend fun onStream(chunk: DataStream, sender: ActrId) {
        val message = if (streamType == StreamType.VOICE) {
             Base64.encodeToString(chunk.payload, Base64.NO_WRAP)
        } else {
             String(chunk.payload, StandardCharsets.UTF_8)
        }

        val metadataArray = Arguments.createArray()
        chunk.metadata.forEach { entry ->
            val map = Arguments.createMap()
            map.putString(contract.keyMetadataKey, entry.key)
            map.putString(contract.keyMetadataValue, entry.value)
            metadataArray.pushMap(map)
        }

        val bridge = ActrBridge.getInstance()
        if (bridge == null) {
            // Log error or throw exception depending on requirement. Swift threw ActrError.
            // For now we just return or log.
            System.err.println("[StreamPrintCallback] Failed to get ActrBridge instance to send event")
            return
        }

        val body = Arguments.createMap()
        body.putString(contract.keyStreamIdUpper, this.streamId)
        body.putString(contract.keyType, this.streamType.wireValue(contract))
        body.putInt(contract.keyStatus, contract.streamStatusOk)
        body.putString(contract.keyMessage, message)
        body.putArray(contract.keyMetadata, metadataArray)
        body.putDouble(contract.keySequence, chunk.sequence.toDouble())

        if (chunk.timestampMs != null) {
             body.putDouble(contract.keyTimestampMs, chunk.timestampMs!!.toDouble())
        }

        bridge.sendStreamEventToRN(body)
    }
}
