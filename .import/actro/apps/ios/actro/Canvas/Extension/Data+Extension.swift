import Foundation
import zlib

extension Data {
    func crc32() -> UInt32 {
        return self.withUnsafeBytes { buffer in
            let base = buffer.bindMemory(to: Bytef.self).baseAddress
            // 显式转换为 UInt32（zlib.crc32 返回 uLong）
            return UInt32(truncatingIfNeeded: zlib.crc32(0, base, uInt(self.count)))
        }
    }
}
