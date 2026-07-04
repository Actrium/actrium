package io.actrium.actro.core.asr

import com.hankcs.hanlp.HanLP
import com.hankcs.hanlp.seg.common.Term
import io.actrium.actro.rust.ActroCore
import io.actrium.actro.rust.AsrSegmentInfo

/**
 * ASR 识别文本后处理器：去除冗余空格 + 自动添加中文标点。
 *
 * 处理流程：
 * 1. 使用 HanLP 分词 + 词性标注，将 Vosk 输出的原始文本重新分词
 * 2. 将分词结果交给 ActroCore，复用 Rust 侧中文拼接和标点规则
 */
object TextPostProcessor {
    fun process(text: String): String {
        if (text.isBlank()) return text
        val terms: List<Term> = HanLP.segment(text.replace(" ", ""))
        if (terms.isEmpty()) return text

        return ActroCore.postProcessZhAsrSegments(
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
