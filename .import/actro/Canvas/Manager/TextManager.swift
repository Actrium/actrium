import Foundation
import SwiftUI
import Combine

/// 全局文案管理类
/// 使用单例模式管理应用中的所有本地化文本
class TextManager: ObservableObject {
    // 单例实例
    static let shared = TextManager()
    
    // 语言管理器引用
    private let languageManager = LanguageManager.shared
    
    private init() {}
    
    // MARK: - 通用文本获取方法
    
    /// 通过键获取本地化文本
    /// - Parameter key: 本地化文本的键
    /// - Returns: 本地化文本
    func text(_ key: TextKey) -> String {
        return languageManager.localizedString(forKey: key.rawValue)
    }
    
    // MARK: - 特定场景文案获取
    
    /// 获取菜单页面文案
    var menu: MenuText {
        return MenuText()
    }
    
    /// 获取输入相关文案
    var input: InputText {
        return InputText()
    }
    
    /// 获取位置相关文案
    var location: LocationText {
        return LocationText()
    }
    
    /// 获取工具栏相关文案
    var toolbar: ToolbarText {
        return ToolbarText()
    }
    
    /// 获取反馈相关文案
    var feedback: FeedbackText {
        return FeedbackText()
    }
    
    /// 获取Toast通知相关文案
    var toast: ToastText {
        return ToastText()
    }
    
    /// 获取其他通用文案
    var common: CommonText {
        return CommonText()
    }
    
    /// 获取天气相关文案
    var weather: WeatherText {
        return WeatherText()
    }
    
    /// 获取日期相关文案
    var date: DateText {
        return DateText()
    }
    
    /// 获取导航相关文案
    var navigation: NavigationText {
        return NavigationText()
    }
    
    /// 获取权限相关文案
    var permission: PermissionText {
        return PermissionText()
    }
    
    /// 获取兜底页相关文案
    var fallback: FallbackText {
        return FallbackText()
    }
    
    /// 获取应用列表页相关文案
    var channels: ChannelsText {
        return ChannelsText()
    }
    
    /// 获取应用列表页相关文案
    var policy: PrivatePolicy {
        return PrivatePolicy()
    }
    
    var about: AboutUs {
        return AboutUs()
    }
    
    var login: Login {
        return Login()
    }
    
    var active: StartActive {
        return StartActive()
    }
}

/// MARK: - 文案键枚举
/// 所有文案键的枚举
enum TextKey: String {
    // 菜单页
    case welcome
	case welcomeTip
    case language
    case about
    case aboutDescription
	case clearChat
	case clearChatTip
	case teasing
	case confirm
	case disagree
	case teasingTopic
    
    // 输入框文案
    case messageOrHoldToTalk
    case sendMessage
    case holdToTalk
    case releaseToSendSwipeToCancel
    case releaseToCancel
    
    // 位置
	case currentLocation
    case addCurrentLocation
    case enableCompanionMode
	case unenableCompanionMode
	case companionTip
	case companionMode
	case companyStart
	case companyNoStart
	case companyTitle
	case companyWelcome
    
    // 工具栏 - 提问
    case edit
    case copy
    case delete
    case cancel
    case send
	case goSetting
    case more
    
    // 工具栏 - 答案
    case commonQuestions
    case references
    case onlyReferences
    case voiceReading
    case stop
    case like
    case dislike
    
    // 反馈弹窗
    case tellUsHowToImprove
    case inaccurate
    case tooShort
    case other
    case placeholder
    case submit
    case submitSuccess
    case feedbackAndReport
    case feedbackPromt
    case harmfulReport
    case sexualReport
    case privacyReport
    case abuseReport
    case illegalReport
    case falseInfoReport
    case copyrightReport
    
    // Toast通知
    case toastSuccess
    case toastError
    case toastWarning
    case toastInfo
    case copySuccess
    case operationSuccess
    case operationFailed
    case networkError
    case dataLoadFailed
	case generatingVoice
	case liked
	case thanksFeedback
    case noTextDetected
    case appUnExist
    case invalidQRCode
    case unavailable
    case noNetwork
    case deleteMessageTitle
    case deleteMessageTip
    
    // 其他
    case disconnected
    case greatwall
    
    // 天气状态
	case weather_level
    case weather_clear
    case weather_partly_cloudy
    case weather_mostly_clear
    case weather_cloudy
    case weather_overcast
    
    // 风力情况
    case weather_windy
    case weather_calm
    case weather_light_breeze
    case weather_moderate_breeze
    case weather_fresh_breeze
    case weather_strong_wind
    case weather_high_wind
    case weather_gale
    case weather_strong_gale
    case weather_storm
    case weather_violent_storm
    case weather_hurricane
    case weather_tropical_storm
    
    // 空气质量
    case weather_haze
    case weather_moderate_haze
    case weather_severe_haze
    case weather_extreme_haze
    
    // 降水情况
    case weather_shower
    case weather_thundershower
    case weather_thundershower_with_hail
    case weather_light_rain
    case weather_moderate_rain
    case weather_heavy_rain
    case weather_torrential_rain
    case weather_downpour
    case weather_extreme_downpour
    case weather_heavy_shower
    case weather_heavy_thundershower
    case weather_extreme_rainfall
    case weather_drizzle
    case weather_rain
    case weather_light_to_moderate_rain
    case weather_moderate_to_heavy_rain
    case weather_heavy_to_torrential_rain
    case weather_torrential_rain_to_downpour
    case weather_downpour_to_extreme_downpour
    
    // 雪与混合天气
    case weather_rain_and_snow
    case weather_sleet
    case weather_snow_rain_shower
    case weather_freezing_rain
    case weather_snow
    case weather_snow_shower
    case weather_light_snow
    case weather_moderate_snow
    case weather_heavy_snow
    case weather_snowstorm
    case weather_light_to_moderate_snow
    case weather_moderate_to_heavy_snow
    case weather_heavy_snow_to_snowstorm
    
    // 沙尘与特殊天气
    case weather_dust
    case weather_blowing_dust
    case weather_dust_storm
    case weather_strong_dust_storm
    case weather_tornado
    
    // 雾
    case weather_fog
    case weather_dense_fog
    case weather_thick_fog
    case weather_mist
    case weather_heavy_fog
    case weather_extreme_dense_fog
    
    // 温度感受
    case weather_hot
    case weather_cold
    
    // 其他天气
    case weather_unknown
	
	// 日期相关
	case today
	case monday
	case tuesday
	case wednesday
	case thursday
	case friday
	case saturday
	case sunday
    
    // 导航相关
    case startNavigation
    case selelctNavigation
    case tencentMap
    case baiduMap
    case amap
    case maps
	case details
	
	// 权限弹窗
	case accuracyTitle
	case accuracyMessage
	case locationTitle
	case locationMessage
	case recordTitle
	case recordMessage
    case cameraTitle
    case cameraMessage
    case photoTitle
    case photoMessage
	
	// 兜底页
    case scanToEnter
    case alignCenter
    
    // 应用列表页
    case appList
    case appActive
    case appUnavailable
    case appRecently
    
    // 隐私协议
    case privateTitle
    case privateMessage
    case privateDisagree
    case privateAgree
    
    // 关于我们
    case aboutUs
    case aboutApp
    case icpInfo
    case aiAndLargeModels
    case feedback
    case privacyPolicy
    case userAgreement
    
    // 登录
    case loginAlertTitle
    case loginAlertMessage
    case loginTip
    case loginByWechat
    case loginByApple
    case authorized
    case authorTip
    case authorExplain
    case declineAuthor
    case agreeAuthor
    case agreeAuthorTip
    case wechatIdentity
    case appleIdentity
    case allowAuthor
    case login
    case loginOut
    case deleteAccount
    case loginOutTitle
    case loginOutConfirm
    case loginOutCancel
    case deleteAccountTitle
    case deleteAccountMessage
    case deleteAccountConfirm
    case deleteAccountCancel
    case nickname
    case email
    case camera
    case photo
    case file
    
    // 起始页
    case activity
    case preferences
    case bookmarks
    case allTag
    case addTagTitle
    case addTagPlaceholder
    case collected
    case canAddTag
    case renameTag
    case deleteTag
    case search
    case deleteTagTitle
    case deleteTagMessage
    case gotIt
    case sendKey
    case sendKeyDescription
    case onlineSearch
    case onlineSearchDescription
    case fileCoveredTitle
    case agreeCover
    case fileTooLarge
    case filelimit
    case fileTypeUnsupported
    case addChannel
    case imagelimit
    case location
    case locationDescription
    case goback
}

// MARK: - 场景文案结构体

/// 菜单页面文案
struct MenuText {
    func welcome() -> String { return TextManager.shared.text(.welcome) }
    func welcomeTip() -> String { return TextManager.shared.text(.welcomeTip) }
    func language() -> String { return TextManager.shared.text(.language) }
    func about() -> String { return TextManager.shared.text(.about) }
    func aboutDescription() -> String { return TextManager.shared.text(.aboutDescription) }
	func clearChat() -> String { return TextManager.shared.text(.clearChat) }
	func clearChatTip() -> String { return TextManager.shared.text(.clearChatTip) }
	func teasing() -> String { return TextManager.shared.text(.teasing) }
	func confirm() -> String { return TextManager.shared.text(.confirm) }
	func disagree() -> String { return TextManager.shared.text(.disagree) }
	func teasingTopic() -> String { return TextManager.shared.text(.teasingTopic) }
}

/// 输入相关文案
struct InputText {
    func messageOrHoldToTalk() -> String { return TextManager.shared.text(.messageOrHoldToTalk) }
    func sendMessage() -> String { return TextManager.shared.text(.sendMessage) }
    func holdToTalk() -> String { return TextManager.shared.text(.holdToTalk) }
    func releaseToSendSwipeToCancel() -> String { return TextManager.shared.text(.releaseToSendSwipeToCancel) }
    func releaseToCancel() -> String { return TextManager.shared.text(.releaseToCancel) }
}

/// 位置相关文案
struct LocationText {
	func currentLocation() -> String { return TextManager.shared.text(.currentLocation)}
    func addCurrentLocation() -> String { return TextManager.shared.text(.addCurrentLocation) }
    func enableCompanionMode() -> String { return TextManager.shared.text(.enableCompanionMode) }
	func unenableCompanionMode() -> String { return TextManager.shared.text(.unenableCompanionMode) }
	func companionTip() -> String { return TextManager.shared.text(.companionTip) }
	func companionMode() -> String { return TextManager.shared.text(.companionMode) }
	func companyTitle() -> String { return TextManager.shared.text(.companyTitle) }
	func companyWelcome() -> String { return TextManager.shared.text(.companyWelcome) }
	func companyStart() -> String { return TextManager.shared.text(.companyStart) }
	func companyNoStart() -> String { return TextManager.shared.text(.companyNoStart) }
}

/// 工具栏相关文案
struct ToolbarText {
    // 提问相关
    func edit() -> String { return TextManager.shared.text(.edit) }
    func copy() -> String { return TextManager.shared.text(.copy) }
    func delete() -> String { return TextManager.shared.text(.delete) }
    func cancel() -> String { return TextManager.shared.text(.cancel) }
    func send() -> String { return TextManager.shared.text(.send) }
	func goSetting() -> String { return TextManager.shared.text(.goSetting) }
    func more() -> String { return TextManager.shared.text(.more) }
    
    // 答案相关
    func commonQuestions() -> String { return TextManager.shared.text(.commonQuestions) }
    func references() -> String { return TextManager.shared.text(.references) }
    func onlyReferences() -> String { return TextManager.shared.text(.onlyReferences) }
    func voiceReading() -> String { return TextManager.shared.text(.voiceReading) }
    func stop() -> String { return TextManager.shared.text(.stop) }
    func like() -> String { return TextManager.shared.text(.like) }
    func dislike() -> String { return TextManager.shared.text(.dislike) }
}

/// 反馈相关文案
struct FeedbackText {
    func tellUsHowToImprove() -> String { return TextManager.shared.text(.tellUsHowToImprove) }
    func inaccurate() -> String { return TextManager.shared.text(.inaccurate) }
    func tooShort() -> String { return TextManager.shared.text(.tooShort) }
    func other() -> String { return TextManager.shared.text(.other) }
    func placeholder() -> String { return TextManager.shared.text(.placeholder) }
    func submit() -> String { return TextManager.shared.text(.submit) }
	func submitSuccess() -> String { return TextManager.shared.text(.submitSuccess) }
    func feedbackAndReport() -> String { return TextManager.shared.text(.feedbackAndReport) }
    func feedbackPromt() -> String { return TextManager.shared.text(.feedbackPromt) }
    func harmfulReport() -> String { return TextManager.shared.text(.harmfulReport) }
    func sexualReport() -> String { return TextManager.shared.text(.sexualReport) }
    func privacyReport() -> String { return TextManager.shared.text(.privacyReport) }
    func abuseReport() -> String { return TextManager.shared.text(.abuseReport) }
    func illegalReport() -> String { return TextManager.shared.text(.illegalReport) }
    func falseInfoReport() -> String { return TextManager.shared.text(.falseInfoReport) }
    func copyrightReport() -> String { return TextManager.shared.text(.copyrightReport) }
}

/// Toast通知相关文案
struct ToastText {
    func success() -> String { return TextManager.shared.text(.toastSuccess) }
    func error() -> String { return TextManager.shared.text(.toastError) }
    func warning() -> String { return TextManager.shared.text(.toastWarning) }
    func info() -> String { return TextManager.shared.text(.toastInfo) }
    func copySuccess() -> String { return TextManager.shared.text(.copySuccess) }
    func operationSuccess() -> String { return TextManager.shared.text(.operationSuccess) }
    func operationFailed() -> String { return TextManager.shared.text(.operationFailed) }
    func networkError() -> String { return TextManager.shared.text(.networkError) }
    func dataLoadFailed() -> String { return TextManager.shared.text(.dataLoadFailed) }
	func generatingVoice() -> String { return TextManager.shared.text(.generatingVoice) }
	func liked() -> String { return TextManager.shared.text(.liked) }
	func thanksFeedback() -> String { return TextManager.shared.text(.thanksFeedback) }
    func noTextDetected() -> String { return TextManager.shared.text(.noTextDetected) }
    func appUnExist() -> String { return TextManager.shared.text(.appUnExist) }
    func invalidQRCode() -> String { return TextManager.shared.text(.invalidQRCode) }
    func unavailable() -> String { return TextManager.shared.text(.unavailable) }
    func noNetwork() -> String { return TextManager.shared.text(.noNetwork) }
    func deleteMessageTitle() -> String { return TextManager.shared.text(.deleteMessageTitle) }
    func deleteMessageTip() -> String { return TextManager.shared.text(.deleteMessageTip) }
}

/// 天气相关文案
struct WeatherText {
    // MARK: - 基本天气状态
	func level() -> String { return TextManager.shared.text(.weather_level) }
    func clear() -> String { return TextManager.shared.text(.weather_clear) }
    func partlyCloudy() -> String { return TextManager.shared.text(.weather_partly_cloudy) }
    func mostlyClear() -> String { return TextManager.shared.text(.weather_mostly_clear) }
    func cloudy() -> String { return TextManager.shared.text(.weather_cloudy) }
    func overcast() -> String { return TextManager.shared.text(.weather_overcast) }
    
    // MARK: - 风力情况
    func windy() -> String { return TextManager.shared.text(.weather_windy) }
    func calm() -> String { return TextManager.shared.text(.weather_calm) }
    func lightBreeze() -> String { return TextManager.shared.text(.weather_light_breeze) }
    func moderateBreeze() -> String { return TextManager.shared.text(.weather_moderate_breeze) }
    func freshBreeze() -> String { return TextManager.shared.text(.weather_fresh_breeze) }
    func strongWind() -> String { return TextManager.shared.text(.weather_strong_wind) }
    func highWind() -> String { return TextManager.shared.text(.weather_high_wind) }
    func gale() -> String { return TextManager.shared.text(.weather_gale) }
    func strongGale() -> String { return TextManager.shared.text(.weather_strong_gale) }
    func storm() -> String { return TextManager.shared.text(.weather_storm) }
    func violentStorm() -> String { return TextManager.shared.text(.weather_violent_storm) }
    func hurricane() -> String { return TextManager.shared.text(.weather_hurricane) }
    func tropicalStorm() -> String { return TextManager.shared.text(.weather_tropical_storm) }
    
    // MARK: - 空气质量
    func haze() -> String { return TextManager.shared.text(.weather_haze) }
    func moderateHaze() -> String { return TextManager.shared.text(.weather_moderate_haze) }
    func severeHaze() -> String { return TextManager.shared.text(.weather_severe_haze) }
    func extremeHaze() -> String { return TextManager.shared.text(.weather_extreme_haze) }
    
    // MARK: - 降水情况
    func shower() -> String { return TextManager.shared.text(.weather_shower) }
    func thundershower() -> String { return TextManager.shared.text(.weather_thundershower) }
    func thundershowerWithHail() -> String { return TextManager.shared.text(.weather_thundershower_with_hail) }
    func lightRain() -> String { return TextManager.shared.text(.weather_light_rain) }
    func moderateRain() -> String { return TextManager.shared.text(.weather_moderate_rain) }
    func heavyRain() -> String { return TextManager.shared.text(.weather_heavy_rain) }
    func torrentialRain() -> String { return TextManager.shared.text(.weather_torrential_rain) }
    func downpour() -> String { return TextManager.shared.text(.weather_downpour) }
    func extremeDownpour() -> String { return TextManager.shared.text(.weather_extreme_downpour) }
    func heavyShower() -> String { return TextManager.shared.text(.weather_heavy_shower) }
    func heavyThundershower() -> String { return TextManager.shared.text(.weather_heavy_thundershower) }
    func extremeRainfall() -> String { return TextManager.shared.text(.weather_extreme_rainfall) }
    func drizzle() -> String { return TextManager.shared.text(.weather_drizzle) }
    func rain() -> String { return TextManager.shared.text(.weather_rain) }
    func lightToModerateRain() -> String { return TextManager.shared.text(.weather_light_to_moderate_rain) }
    func moderateToHeavyRain() -> String { return TextManager.shared.text(.weather_moderate_to_heavy_rain) }
    func heavyToTorrentialRain() -> String { return TextManager.shared.text(.weather_heavy_to_torrential_rain) }
    func torrentialRainToDownpour() -> String { return TextManager.shared.text(.weather_torrential_rain_to_downpour) }
    func downpourToExtremeDownpour() -> String { return TextManager.shared.text(.weather_downpour_to_extreme_downpour) }
    
    // MARK: - 雪与混合天气
    func rainAndSnow() -> String { return TextManager.shared.text(.weather_rain_and_snow) }
    func sleet() -> String { return TextManager.shared.text(.weather_sleet) }
    func snowRainShower() -> String { return TextManager.shared.text(.weather_snow_rain_shower) }
    func freezingRain() -> String { return TextManager.shared.text(.weather_freezing_rain) }
    func snow() -> String { return TextManager.shared.text(.weather_snow) }
    func snowShower() -> String { return TextManager.shared.text(.weather_snow_shower) }
    func lightSnow() -> String { return TextManager.shared.text(.weather_light_snow) }
    func moderateSnow() -> String { return TextManager.shared.text(.weather_moderate_snow) }
    func heavySnow() -> String { return TextManager.shared.text(.weather_heavy_snow) }
    func snowstorm() -> String { return TextManager.shared.text(.weather_snowstorm) }
    func lightToModerateSnow() -> String { return TextManager.shared.text(.weather_light_to_moderate_snow) }
    func moderateToHeavySnow() -> String { return TextManager.shared.text(.weather_moderate_to_heavy_snow) }
    func heavySnowToSnowstorm() -> String { return TextManager.shared.text(.weather_heavy_snow_to_snowstorm) }
    
    // MARK: - 沙尘与特殊天气
    func dust() -> String { return TextManager.shared.text(.weather_dust) }
    func blowingDust() -> String { return TextManager.shared.text(.weather_blowing_dust) }
    func dustStorm() -> String { return TextManager.shared.text(.weather_dust_storm) }
    func strongDustStorm() -> String { return TextManager.shared.text(.weather_strong_dust_storm) }
    func tornado() -> String { return TextManager.shared.text(.weather_tornado) }
    
    // MARK: - 雾
    func fog() -> String { return TextManager.shared.text(.weather_fog) }
    func denseFog() -> String { return TextManager.shared.text(.weather_dense_fog) }
    func thickFog() -> String { return TextManager.shared.text(.weather_thick_fog) }
    func mist() -> String { return TextManager.shared.text(.weather_mist) }
    func heavyFog() -> String { return TextManager.shared.text(.weather_heavy_fog) }
    func extremeDenseFog() -> String { return TextManager.shared.text(.weather_extreme_dense_fog) }
    
    // MARK: - 温度感受
    func hot() -> String { return TextManager.shared.text(.weather_hot) }
    func cold() -> String { return TextManager.shared.text(.weather_cold) }
    
    // MARK: - 其他
    func unknown() -> String { return TextManager.shared.text(.weather_unknown) }
    
    // MARK: - 辅助方法
    
    /// 根据天气中文描述获取本地化的天气描述
    /// - Parameter code: 天气中文描述
    /// - Returns: 本地化的天气描述
    func getWeatherDescription(for chineseText: String) -> String {
        switch chineseText {
        // 基本天气状态
        case "晴": return clear()
        case "少云": return partlyCloudy()
        case "晴间多云": return mostlyClear()
        case "多云": return cloudy()
        case "阴": return overcast()
            
        // 风力情况
        case "有风": return windy()
        case "平静": return calm()
        case "微风": return lightBreeze()
        case "和风": return moderateBreeze()
        case "清风": return freshBreeze()
        case "强风", "劲风": return strongWind()
        case "疾风": return highWind()
        case "大风": return gale()
        case "烈风": return strongGale()
        case "风暴": return storm()
        case "狂爆风": return violentStorm()
        case "飓风": return hurricane()
        case "热带风暴": return tropicalStorm()
            
        // 空气质量
        case "霾": return haze()
        case "中度霾": return moderateHaze()
        case "重度霾": return severeHaze()
        case "严重霾": return extremeHaze()
            
        // 降水情况
        case "阵雨": return shower()
        case "雷阵雨": return thundershower()
        case "雷阵雨并伴有冰雹": return thundershowerWithHail()
        case "小雨": return lightRain()
        case "中雨": return moderateRain()
        case "大雨": return heavyRain()
        case "暴雨": return torrentialRain()
        case "大暴雨": return downpour()
        case "特大暴雨": return extremeDownpour()
        case "强阵雨": return heavyShower()
        case "强雷阵雨": return heavyThundershower()
        case "极端降雨": return extremeRainfall()
        case "毛毛雨", "细雨": return drizzle()
        case "雨": return rain()
        case "小雨-中雨": return lightToModerateRain()
        case "中雨-大雨": return moderateToHeavyRain()
        case "大雨-暴雨": return heavyToTorrentialRain()
        case "暴雨-大暴雨": return torrentialRainToDownpour()
        case "大暴雨-特大暴雨": return downpourToExtremeDownpour()
            
        // 雪与混合天气
        case "雨雪天气": return rainAndSnow()
        case "雨夹雪": return sleet()
        case "阵雨夹雪": return snowRainShower()
        case "冻雨": return freezingRain()
        case "雪": return snow()
        case "阵雪": return snowShower()
        case "小雪": return lightSnow()
        case "中雪": return moderateSnow()
        case "大雪": return heavySnow()
        case "暴雪": return snowstorm()
        case "小雪-中雪": return lightToModerateSnow()
        case "中雪-大雪": return moderateToHeavySnow()
        case "大雪-暴雪": return heavySnowToSnowstorm()
            
        // 沙尘与特殊天气
        case "浮尘": return dust()
        case "扬沙": return blowingDust()
        case "沙尘暴": return dustStorm()
        case "强沙尘暴": return strongDustStorm()
        case "龙卷风": return tornado()
            
        // 雾
        case "雾": return fog()
        case "浓雾": return denseFog()
        case "强浓雾": return thickFog()
        case "轻雾": return mist()
        case "大雾": return heavyFog()
        case "特强浓雾": return extremeDenseFog()
            
        // 温度感受
        case "热": return hot()
        case "冷": return cold()
            
        // 默认情况或未知天气
        default: return unknown()
        }
    }
}

/// 日期相关文案
struct DateText {
    /// 获取"今天"的本地化文案
    func today() -> String { 
        return TextManager.shared.text(.today) 
    }
    
    /// 获取周一的本地化文案
    func monday() -> String { 
        return TextManager.shared.text(.monday) 
    }
    
    /// 获取周二的本地化文案
    func tuesday() -> String { 
        return TextManager.shared.text(.tuesday) 
    }
    
    /// 获取周三的本地化文案
    func wednesday() -> String { 
        return TextManager.shared.text(.wednesday) 
    }
    
    /// 获取周四的本地化文案
    func thursday() -> String { 
        return TextManager.shared.text(.thursday) 
    }
    
    /// 获取周五的本地化文案
    func friday() -> String { 
        return TextManager.shared.text(.friday) 
    }
    
    /// 获取周六的本地化文案
    func saturday() -> String { 
        return TextManager.shared.text(.saturday) 
    }
    
    /// 获取周日的本地化文案
    func sunday() -> String { 
        return TextManager.shared.text(.sunday) 
    }
    
    /// 根据星期几的索引获取对应的本地化文案
    /// - Parameter weekday: 星期几索引 (1-7, 其中1代表周日，7代表周六)
    /// - Returns: 对应星期几的本地化文案
    func weekday(_ weekday: Int) -> String {
        switch weekday {
        case 1: return sunday()
        case 2: return monday()
        case 3: return tuesday()
        case 4: return wednesday()
        case 5: return thursday()
        case 6: return friday()
        case 7: return saturday()
        default: return ""
        }
    }
    
    /// 根据日期获取对应的星期几文案
    /// - Parameter date: 日期
    /// - Returns: 星期几的本地化文案
    func weekdayName(for date: Date) -> String {
        let calendar = Calendar.current
        let weekday = calendar.component(.weekday, from: date)
        return self.weekday(weekday)
    }
    
    /// 获取当前日期是否为"今天"，如果是则返回今天的本地化文案，否则返回对应的星期几
    /// - Parameter date: 要检查的日期
    /// - Returns: "今天"或对应的星期几本地化文案
    func dateString(for date: Date) -> String {
        let calendar = Calendar.current
        
        if calendar.isDateInToday(date) {
            return today()
        } else {
            return weekdayName(for: date)
        }
    }
    
    /// 格式化并本地化日期
    /// - Parameters:
    ///   - date: 要格式化的日期
    ///   - showWeekday: 是否显示星期
    /// - Returns: 格式化后的日期字符串
    func formattedDate(date: Date, showWeekday: Bool = true) -> String {
        let dateFormatter = DateFormatter()
        dateFormatter.dateStyle = .medium
        let dateString = dateFormatter.string(from: date)
        
        if showWeekday {
            return "\(dateString), \(weekdayName(for: date))"
        } else {
            return dateString
        }
    }
}
/// 其他通用文案
struct CommonText {
    func disconnected() -> String { return TextManager.shared.text(.disconnected) }
	func greatwall() -> String { return TextManager.shared.text(.greatwall) }
}

/// 导航相关文案
struct NavigationText {
    func startNavigation() -> String { return TextManager.shared.text(.startNavigation) }
    func selectNavigation() -> String { return TextManager.shared.text(.selelctNavigation) }
    func tencentMap() -> String { return TextManager.shared.text(.tencentMap) }
    func baiduMap() -> String { return TextManager.shared.text(.baiduMap) }
    func amap() -> String { return TextManager.shared.text(.amap) }
    func maps() -> String { return TextManager.shared.text(.maps) }
	func details() -> String { return TextManager.shared.text(.details) }
}

/// 权限相关文案
struct PermissionText {
    func accuracyTitle() -> String { return TextManager.shared.text(.accuracyTitle) }
    func accuracyMessage() -> String { return TextManager.shared.text(.accuracyMessage) }
    func locationTitle() -> String { return TextManager.shared.text(.locationTitle) }
    func locationMessage() -> String { return TextManager.shared.text(.locationMessage) }
    func recordTitle() -> String { return TextManager.shared.text(.recordTitle) }
    func recordMessage() -> String { return TextManager.shared.text(.recordMessage) }
    func cameraTitle() -> String { return TextManager.shared.text(.cameraTitle) }
    func cameraMessage() -> String { return TextManager.shared.text(.cameraMessage) }
    func photoTitle() -> String { return TextManager.shared.text(.photoTitle) }
    func photoMessage() -> String { return TextManager.shared.text(.photoMessage) }
}

/// 兜底页相关文案
struct FallbackText {
    func scanToEnter() -> String { return TextManager.shared.text(.scanToEnter) }
    func alignCenter() -> String { return TextManager.shared.text(.alignCenter) }
}

/// 应用列表页相关文案
struct ChannelsText {
    func appList() -> String { return TextManager.shared.text(.appList) }
    func appActive() -> String { return TextManager.shared.text(.appActive) }
    func appUnavailable() -> String { return TextManager.shared.text(.appUnavailable) }
    func appRecently() -> String { return TextManager.shared.text(.appRecently) }
}

/// 隐私协议相关文案
struct PrivatePolicy {
    func privateTitle() -> String { return TextManager.shared.text(.privateTitle) }
    func privateMessage() -> String { return TextManager.shared.text(.privateMessage) }
    func privateDisagree() -> String { return TextManager.shared.text(.privateDisagree) }
    func privateAgree() -> String { return TextManager.shared.text(.privateAgree) }
}

/// 关于我们
struct AboutUs {
    func aboutUs() -> String { return TextManager.shared.text(.aboutUs) }
    func aboutApp() -> String { return TextManager.shared.text(.aboutApp) }
    func icpInfo() -> String { return TextManager.shared.text(.icpInfo) }
    func aiAndLargeModels() -> String { return TextManager.shared.text(.aiAndLargeModels) }
    func feedback() -> String { return TextManager.shared.text(.feedback) }
    func privacyPolicy() -> String { return TextManager.shared.text(.privacyPolicy) }
    func userAgreement() -> String { return TextManager.shared.text(.userAgreement) }
}

/// 登录
struct Login {
    func loginAlertTitle() -> String { return TextManager.shared.text(.loginAlertTitle) }
    func loginAlertMessage() -> String { return TextManager.shared.text(.loginAlertMessage) }
    func loginTip() -> String { return TextManager.shared.text(.loginTip) }
    func loginByWechat() -> String { return TextManager.shared.text(.loginByWechat) }
    func loginByApple() -> String { return TextManager.shared.text(.loginByApple) }
    func authorized() -> String { return TextManager.shared.text(.authorized) }
    func authorTip() -> String { return TextManager.shared.text(.authorTip) }
    func authorExplain() -> String { return TextManager.shared.text(.authorExplain) }
    func declineAuthor() -> String { return TextManager.shared.text(.declineAuthor) }
    func agreeAuthor() -> String { return TextManager.shared.text(.agreeAuthor) }
    func agreeAuthorTip() -> String { return TextManager.shared.text(.agreeAuthorTip) }
    func wechatIdentity() -> String { return TextManager.shared.text(.wechatIdentity) }
    func appleIdentity() -> String { return TextManager.shared.text(.appleIdentity) }
    func allowAuthor() -> String { return TextManager.shared.text(.allowAuthor) }
    func login() -> String { return TextManager.shared.text(.login) }
    func loginOut() -> String { return TextManager.shared.text(.loginOut) }
    func deleteAccount() -> String { return TextManager.shared.text(.deleteAccount) }
    func loginOutTitle() -> String { return TextManager.shared.text(.loginOutTitle) }
    func loginOutConfirm() -> String { return TextManager.shared.text(.loginOutConfirm) }
    func loginOutCancel() -> String { return TextManager.shared.text(.loginOutCancel) }
    func deleteAccountTitle() -> String { return TextManager.shared.text(.deleteAccountTitle) }
    func deleteAccountMessage() -> String { return TextManager.shared.text(.deleteAccountMessage) }
    func deleteAccountConfirm() -> String { return TextManager.shared.text(.deleteAccountConfirm) }
    func deleteAccountCancel() -> String { return TextManager.shared.text(.deleteAccountCancel) }
    func nickname() -> String { return TextManager.shared.text(.nickname) }
    func email() -> String { return TextManager.shared.text(.email) }
    func camera() -> String { return TextManager.shared.text(.camera) }
    func photo() -> String { return TextManager.shared.text(.photo) }
    func file() -> String { return TextManager.shared.text(.file) }
}

struct StartActive {
    func activity() -> String { return TextManager.shared.text(.activity) }
    func preferences() -> String { return TextManager.shared.text(.preferences) }
    func bookmarks() -> String { return TextManager.shared.text(.bookmarks) }
    func allTag() -> String { return TextManager.shared.text(.allTag) }
    func addTagTitle() -> String { return TextManager.shared.text(.addTagTitle) }
    func addTagPlaceholder() -> String { return TextManager.shared.text(.addTagPlaceholder) }
    func collected() -> String { return TextManager.shared.text(.collected) }
    func canAddTag() -> String { return TextManager.shared.text(.canAddTag) }
    func renameTag() -> String { return TextManager.shared.text(.renameTag) }
    func deleteTag() -> String { return TextManager.shared.text(.deleteTag) }
    func search() -> String { return TextManager.shared.text(.search) }
    func deleteTagTitle() -> String { return TextManager.shared.text(.deleteTagTitle) }
    func deleteTagMessage() -> String { return TextManager.shared.text(.deleteTagMessage) }
    func gotIt() -> String { return TextManager.shared.text(.gotIt) }
    func sendKey() -> String { return TextManager.shared.text(.sendKey) }
    func sendKeyDescription() -> String { return TextManager.shared.text(.sendKeyDescription) }
    func onlineSearch() -> String { return TextManager.shared.text(.onlineSearch) }
    func onlineSearchDescription() -> String { return TextManager.shared.text(.onlineSearchDescription) }
    func fileCoveredTitle() -> String { return TextManager.shared.text(.fileCoveredTitle) }
    func agreeCover() -> String { return TextManager.shared.text(.agreeCover) }
    func fileTooLarge() -> String { return TextManager.shared.text(.fileTooLarge) }
    func filelimit() -> String { return TextManager.shared.text(.filelimit) }
    func fileTypeUnsupported() -> String { return TextManager.shared.text(.fileTypeUnsupported) }
    func addChannel() -> String { return TextManager.shared.text(.addChannel) }
    func imagelimit() -> String { return TextManager.shared.text(.imagelimit) }
    func location() -> String { return TextManager.shared.text(.location) }
    func locationDescription() -> String { return TextManager.shared.text(.locationDescription) }
    func goback() -> String { return TextManager.shared.text(.goback) }
}

// MARK: - SwiftUI文本扩展
extension View {
    /// 方便在SwiftUI中使用本地化文本的方法
    func localizedText(_ key: TextKey) -> Text {
        return Text(TextManager.shared.text(key))
    }
}

// MARK: - 便捷访问
/// 提供全局访问点，方便代码中使用
let TM = TextManager.shared
