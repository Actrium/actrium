package io.actrium.actro.core

import io.actrium.actro.rust.ActroCore

object UserInfoBridgeParams {
    private val userInfoContract = ActroCore.userInfoBridgeContract()

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

    val loginResultEventName = ActroCore.runtimeLoginResultEventName()
    val loginResultTypeKey = ActroCore.runtimeLoginResultTypeKey()
    val loginResultCodeKey = ActroCore.runtimeLoginResultCodeKey()
    val loginResultIdTokenKey = ActroCore.runtimeLoginResultIdTokenKey()
    val loginResultErrorKey = ActroCore.runtimeLoginResultErrorKey()
    val loginResultMessageKey = ActroCore.runtimeLoginResultMessageKey()
    val loginTypeWechat = ActroCore.runtimeLoginTypeWechat()
    val loginTypeGoogle = ActroCore.runtimeLoginTypeGoogle()
}
