package io.actrium.actro.model

import android.os.Bundle
import io.actrium.actro.utils.TabBundleUtils

data class AppModel(
    val unifiedScript: String = "", // guest远程路径， 暂时为空字符串
    val rightPanelDefaultVisible: Boolean = false, //   右侧是否默认显示
    val id: String = "", // uuid
    val backgroundColor: String = "#FFFFFF", //固定值
    val reloadKey: Int = 0, // 改变触发RN重新渲染
    val htmlContent: String = "",   //iOS使用
    val title: String = ""  // 应用名
) {
    var isLeftPanelVisible = false
    var isRightPanelVisible = false

    fun toBundle(): Bundle {
        return TabBundleUtils.genBundle(this)
    }

    fun genReactNativeBundle(tabs: List<AppModel>, height: Int, width: Int): Bundle {
        return TabBundleUtils.genBundle(this, tabs, height, width)
    }
}
