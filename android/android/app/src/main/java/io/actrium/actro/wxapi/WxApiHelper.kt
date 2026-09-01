package io.actrium.actro.wxapi

import android.content.Context
import com.tencent.mm.opensdk.modelmsg.SendAuth
import com.tencent.mm.opensdk.openapi.IWXAPI
import com.tencent.mm.opensdk.openapi.WXAPIFactory
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.asSharedFlow
import kotlinx.coroutines.launch
import io.actrium.actro.global.Constants

class WxApiHelper private constructor(context: Context) {
    companion object {
        @Volatile
        private var INSTANCE: WxApiHelper? = null

        fun getInstance(context: Context): WxApiHelper {
            return INSTANCE ?: synchronized(this) {
                INSTANCE ?: WxApiHelper(context.applicationContext).also { INSTANCE = it }
            }
        }
    }

    private val api: IWXAPI = WXAPIFactory.createWXAPI(context, Constants.WX_APP_ID, false)

    // Scope for emitting events
    private val scope = CoroutineScope(Dispatchers.Main + SupervisorJob())

    // SharedFlow for authentication codes from WXEntryActivity
    private val _authCode = MutableSharedFlow<String?>(replay = 0)
    val authCode = _authCode.asSharedFlow()
    private val _authErr = MutableSharedFlow<String?>(replay = 0)
    val authErr = _authErr.asSharedFlow()

    fun getWxApi(): IWXAPI {
        return api
    }

    fun initial() {
        api.registerApp(Constants.WX_APP_ID)
    }

    fun login() {
        if (!api.isWXAppInstalled) {
            postAuthErr("No Twitter installed")
            return
        }

        val req = SendAuth.Req()
        req.scope = "snsapi_userinfo"
        req.state = "actro_login"

        if (!api.sendReq(req)) {
            postAuthErr("Failed to pull up micro-input")
        }
    }

    /** Post the received auth code to the SharedFlow for observers to collect. */
    fun postAuthCode(code: String?) {
        scope.launch { _authCode.emit(code) }
    }
    fun postAuthErr(err: String?) {
        scope.launch { _authErr.emit(err) }
    }
}