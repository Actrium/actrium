package io.actr.datastreamapp

import io.actor_rtc.actr.ActrId
import io.actor_rtc.actr.DataStream
import io.actor_rtc.actr.DataStreamCallback
import com.facebook.react.bridge.Arguments
import com.facebook.react.bridge.WritableMap
import com.facebook.react.bridge.WritableArray
import com.askaway.android.core.ActrBridge
import android.util.Base64
import java.nio.charset.StandardCharsets

enum class StreamType {
    TEXT,
    VOICE
}

class AskAwayDataStreamCallback(val streamId:String, val streamType: StreamType): DataStreamCallback{
    override suspend fun onStream(chunk: DataStream, sender: ActrId) {
        val message = if (streamType == StreamType.VOICE) {
             Base64.encodeToString(chunk.payload, Base64.NO_WRAP)
        } else {
             String(chunk.payload, StandardCharsets.UTF_8)
        }

        val metadataArray = Arguments.createArray()
        chunk.metadata.forEach { entry ->
            val map = Arguments.createMap()
            map.putString("key", entry.key)
            map.putString("value", entry.value)
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
        body.putString("streamID", this.streamId)
        body.putInt("type", this.streamType.ordinal)
        body.putInt("status", 0)
        body.putString("message", message)
        body.putArray("metadata", metadataArray)
        body.putInt("sequence", chunk.sequence.toInt()) // Assuming sequence fits in Int or use Double

        if (chunk.timestampMs != null) {
             body.putDouble("timestampMs", chunk.timestampMs!!.toDouble())
        }

        bridge.sendStreamEventToRN(body)
    }
}