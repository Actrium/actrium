import Foundation

extension Double {
	func toFixed(_ decimals: Int) -> String {
		String(format: "%.\(decimals)f", self)
	}
}
