package chat.askaway.android.wxapi

import android.os.Bundle
import android.util.Log
import androidx.activity.ComponentActivity
import com.tencent.mm.opensdk.modelbase.BaseReq
import com.tencent.mm.opensdk.modelbase.BaseResp
import com.tencent.mm.opensdk.modelmsg.SendAuth
import com.tencent.mm.opensdk.openapi.IWXAPI
import com.tencent.mm.opensdk.openapi.IWXAPIEventHandler
import javax.inject.Inject

class WXEntryActivity : ComponentActivity(), IWXAPIEventHandler {

    lateinit var wxApiHelper: WxApiHelper

    private lateinit var api: IWXAPI

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        wxApiHelper = WxApiHelper.getInstance(this)
        api = wxApiHelper.getWxApi()
        api.handleIntent(intent, this)
    }

    override fun onReq(p0: BaseReq?) {

    }

    override fun onResp(p0: BaseResp?) {
        Log.e("WXEntryActivity", "Received WeChat response: type=${p0?.type}, errCode=${p0?.errCode}, errStr=${p0?.errStr}")
        if (p0 is SendAuth.Resp) {
            // 处理微信登录响应
            Log.e("WXEntryActivity", "Received WeChat auth response: code=${p0.code}, state=${p0.state}, errCode=${p0.errCode}")
            if(p0.errCode ==0){
                val code = p0.code
                wxApiHelper.postAuthCode(code)
            }else{
                wxApiHelper.postAuthErr(p0.errStr)
            }

            finish()
        }
    }
}