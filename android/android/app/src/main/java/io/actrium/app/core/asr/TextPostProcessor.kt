package io.actrium.app.core.asr

import com.hankcs.hanlp.HanLP
import com.hankcs.hanlp.seg.common.Term
import io.actrium.app.rust.ActriumCore
import io.actrium.app.rust.AsrSegmentInfo

/**
 * ASR Identification of text reprocessing: remove redundant spaces + Automatically add a Chinese pun.
 *
 * Process:
 * 1. Use HanLP Interword + Word type label, will Vosk Output original text renumbering
 * 2. Give the word-speech to the ActriumCore,Reuse Rust Scrolling and Punctuation Rules
 */
object TextPostProcessor {
    fun process(text: String): String {
        if (text.isBlank()) return text
        val terms: List<Term> = HanLP.segment(text.replace(" ", ""))
        if (terms.isEmpty()) return text

        return ActriumCore.postProcessZhAsrSegments(
            text,
            terms.map { term ->
                AsrSegmentInfo(
                    word = term.word,
                    nature = term.nature?.toString(),
                )
            },
        )
    }
}
