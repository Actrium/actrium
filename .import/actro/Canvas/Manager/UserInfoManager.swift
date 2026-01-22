import Combine

// 存储的用户信息模型
struct UserStorageInfo: Codable, Identifiable {
    let id: Int64
    var appId: String
    var appName: String
    var configUrl: String
    var userInfo: String // JSON string of A1UserModels.UserInfo / UserDetailData
    var userIdentity: String // JSON string of identities
    var sessionToken: String
    var createdAt: Date
    var updatedAt: Date

    init(id: Int64 = 0, appId: String, appName: String = "", configUrl: String = "", userInfo: String = "", userIdentity: String = "", sessionToken: String = "", createdAt: Date = Date(), updatedAt: Date = Date()) {
        self.id = id
        self.appId = appId
        self.appName = appName
        self.configUrl = configUrl
        self.userInfo = userInfo
        self.userIdentity = userIdentity
        self.sessionToken = sessionToken
        self.createdAt = createdAt
        self.updatedAt = updatedAt
    }
}

class UserInfoManager: ObservableObject {
    static let shared = UserInfoManager()

    private init() {}

    func loadAllUsers() {
    }

    @discardableResult
    func addOrUpdateUser(_ user: UserStorageInfo) -> Bool {
        return false
    }

    func getUser() -> UserStorageInfo? {
        return nil
    }

    @discardableResult
    func deleteUser(byAppId appId: String) -> Bool {
        return false
    }
}

