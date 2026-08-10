using System.IO;
using System.Text;

namespace WinUiEchoApp;

// Manual protobuf codec for echo.EchoRequest / echo.EchoResponse.
//
// Schema (e2e/swift-echo-app/protos/remote/echo/echo.proto):
//   message EchoRequest  { string message = 1; }
//   message EchoResponse { string reply = 1; uint64 timestamp = 2; }
//
// Hand-rolled to avoid pulling Google.Protobuf + a protoc build step into the
// e2e fixture. The schema is tiny and stable.
internal static class EchoProto
{
    /// Encode an <c>EchoRequest { message }</c> to protobuf bytes.
    public static byte[] EncodeEchoRequest(string message)
    {
        var msgBytes = Encoding.UTF8.GetBytes(message);
        using var ms = new MemoryStream();
        // field 1, wire type 2 (length-delimited): tag = (1 << 3) | 2 = 0x0a
        WriteVarint(ms, 0x0a);
        WriteVarint(ms, (uint)msgBytes.Length);
        ms.Write(msgBytes, 0, msgBytes.Length);
        return ms.ToArray();
    }

    /// Decode the <c>reply</c> field (field 1, string) from an EchoResponse.
    public static string DecodeEchoReply(byte[] data)
    {
        var reply = string.Empty;
        var i = 0;
        while (i < data.Length)
        {
            var (tag, ni) = ReadVarint(data, i);
            i = ni;
            var field = (int)(tag >> 3);
            var wire = (int)(tag & 7);
            if (field == 1 && wire == 2)
            {
                var (len, ni2) = ReadVarint(data, i);
                i = ni2;
                reply = Encoding.UTF8.GetString(data, i, (int)len);
                i += (int)len;
            }
            else
            {
                i = SkipField(data, i, wire);
            }
        }
        return reply;
    }

    private static void WriteVarint(Stream s, uint v)
    {
        while (v >= 0x80)
        {
            s.WriteByte((byte)(v | 0x80));
            v >>= 7;
        }
        s.WriteByte((byte)v);
    }

    private static (uint value, int next) ReadVarint(byte[] d, int i)
    {
        uint v = 0;
        var shift = 0;
        while (i < d.Length)
        {
            var b = d[i++];
            v |= (uint)(b & 0x7f) << shift;
            if ((b & 0x80) == 0) break;
            shift += 7;
        }
        return (v, i);
    }

    private static int SkipField(byte[] d, int i, int wire) => wire switch
    {
        0 => SkipVarint(d, i),
        1 => i + 8,                 // 64-bit
        2 => ReadVarint(d, i) is (var len, var ni) ? ni + (int)len : i, // length-delimited
        5 => i + 4,                 // 32-bit
        _ => throw new InvalidDataException($"unknown protobuf wire type {wire}"),
    };

    private static int SkipVarint(byte[] d, int i)
    {
        while (i < d.Length && (d[i++] & 0x80) != 0) { }
        return i;
    }
}
