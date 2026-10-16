package io.actrium.app.core

import io.actrium.app.rust.ActriumCore

object UserInfoBridgeParams {
    private val userInfoContract = ActriumCore.userInfoBridgeContract()

    val moduleName = userInfoContract.moduleName
    val eventUserInfoChanged = userInfoContract.eventUserInfoChanged
    val eventLoginStatusChanged = userInfoContract.eventLoginStatusChanged
    val eventUserLogout = userInfoContract.eventUserLogout
    val keyId = userInfoContract.keyId
    val keyLoginApiUrl = userInfoContract.keyLoginApiUrl
    val keyUserInfo = userInfoContract.keyUserInfo
    val keyUserIdentity = userInfoContract.keyUserIdentity
    val keySessionToken = userInfoContract.keySessionToken
    val keyCreatedAt = userInfoContract.keyCreatedAt
    val keyUpdatedAt = userInfoContract.keyUpdatedAt
    val keyRequestContext = userInfoContract.keyRequestContext
    val keyIsLoggedIn = userInfoContract.keyIsLoggedIn

    val loginResultEventName = ActriumCore.runtimeLoginResultEventName()
    val loginResultTypeKey = ActriumCore.runtimeLoginResultTypeKey()
    val loginResultCodeKey = ActriumCore.runtimeLoginResultCodeKey()
    val loginResultIdTokenKey = ActriumCore.runtimeLoginResultIdTokenKey()
    val loginResultErrorKey = ActriumCore.runtimeLoginResultErrorKey()
    val loginResultMessageKey = ActriumCore.runtimeLoginResultMessageKey()
    val loginTypeWechat = ActriumCore.runtimeLoginTypeWechat()
    val loginTypeGoogle = ActriumCore.runtimeLoginTypeGoogle()
}
