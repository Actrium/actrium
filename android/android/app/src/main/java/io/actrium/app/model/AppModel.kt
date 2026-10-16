package io.actrium.app.model

import android.os.Bundle
import io.actrium.app.utils.TabBundleUtils

data class AppModel(
    val unifiedScript: String = "", // guestRemote Path, Other Organiser
    val rightPanelDefaultVisible: Boolean = false, //   Whether the right is shown by default
    val id: String = "", // uuid
    val backgroundColor: String = "#FFFFFF", //Fixed value
    val reloadKey: Int = 0, // Change TriggerRNRender
    val htmlContent: String = "",   //iOSUse
    val title: String = ""  // Apply Name
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
