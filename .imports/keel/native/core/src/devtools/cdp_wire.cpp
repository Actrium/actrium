/*
 * cdp_wire.cpp — implementations of the pure-string CDP JSON wire helpers (see
 * cdp_wire.h). Moved out of CDPServer.cpp verbatim so the QuickJS debug wasm can
 * link them without the networking server.
 *
 * Licensed under the Apache License, Version 2.0.
 */

// WIP subsystem — gated behind KEEL_WIP_CDP_DEVTOOLS (off by default in production
// builds). The debug wasm builds pass -DKEEL_WIP_CDP_DEVTOOLS=1 and need these
// helpers; a normal Apple pod build leaves an empty TU instead of shipping them.
#if KEEL_WIP_CDP_DEVTOOLS

#include "cdp_wire.h"

#include <cctype>
#include <cstdint>
#include <iomanip>
#include <sstream>

namespace keel::devtools {
namespace cdp {

// Quick validation: params must start with '{' and end with '}'. Not a full JSON
// parser, but catches obviously malformed input.
static bool looksLikeJSONObject(const std::string& s) {
  if (s.empty()) return false;
  size_t first = 0;
  while (first < s.size() && std::isspace(static_cast<unsigned char>(s[first]))) first++;
  size_t last = s.size();
  while (last > first && std::isspace(static_cast<unsigned char>(s[last - 1]))) last--;
  return (last > first) && s[first] == '{' && s[last - 1] == '}';
}

std::string buildEventJSON(const std::string& method, const std::string& params,
                           const std::optional<std::string>& sessionId) {
  std::ostringstream ss;
  ss << "{\"method\":\"" << escapeJSON(method) << "\"";
  // Validate params is a JSON object; fallback to empty object
  ss << ",\"params\":" << (looksLikeJSONObject(params) ? params : std::string("{}"));
  if (sessionId) {
    ss << ",\"sessionId\":\"" << escapeJSON(*sessionId) << "\"";
  }
  ss << "}";
  return ss.str();
}

std::string buildResponseJSON(int id, const std::string& result) {
  std::ostringstream ss;
  ss << "{\"id\":" << id;
  ss << ",\"result\":" << result;
  ss << "}";
  return ss.str();
}

std::string buildErrorJSON(int id, int code, const std::string& message) {
  std::ostringstream ss;
  ss << "{\"id\":" << id;
  ss << ",\"error\":{\"code\":" << code;
  ss << ",\"message\":\"" << escapeJSON(message) << "\"}}";
  return ss.str();
}

std::string escapeJSON(const std::string& str) {
  // U+FFFD REPLACEMENT CHARACTER, as raw UTF-8 bytes. Invalid input bytes are
  // substituted with it instead of being passed through: the output of this
  // function is embedded verbatim into wire JSON, and a raw invalid byte there
  // would make the whole CDP message ill-formed UTF-8 (some clients drop the
  // entire frame, taking unrelated messages down with it).
  static const char kReplacement[] = "\xEF\xBF\xBD";

  std::ostringstream ss;
  const size_t n = str.size();
  size_t i = 0;
  while (i < n) {
    unsigned char c = static_cast<unsigned char>(str[i]);
    if (c < 0x80) {  // ASCII: JSON escapes as before
      switch (c) {
        case '"':  ss << "\\\""; break;
        case '\\': ss << "\\\\"; break;
        case '\b': ss << "\\b"; break;
        case '\f': ss << "\\f"; break;
        case '\n': ss << "\\n"; break;
        case '\r': ss << "\\r"; break;
        case '\t': ss << "\\t"; break;
        default:
          if (c < 0x20) {
            ss << "\\u" << std::hex << std::setfill('0') << std::setw(4)
               << static_cast<int>(c) << std::dec;
          } else {
            ss << static_cast<char>(c);
          }
      }
      i++;
      continue;
    }

    // Multi-byte UTF-8 sequence. Validate per RFC 3629 (well-formed byte
    // ranges: no overlongs, no surrogates, max U+10FFFF); anything invalid
    // becomes one U+FFFD per bad byte.
    size_t len = 0;
    unsigned char firstLo = 0x80, firstHi = 0xBF;  // valid range of byte 2
    if (c >= 0xC2 && c <= 0xDF) { len = 2; }
    else if (c == 0xE0)         { len = 3; firstLo = 0xA0; }
    else if (c >= 0xE1 && c <= 0xEC) { len = 3; }
    else if (c == 0xED)         { len = 3; firstHi = 0x9F; }  // exclude surrogates
    else if (c >= 0xEE && c <= 0xEF) { len = 3; }
    else if (c == 0xF0)         { len = 4; firstLo = 0x90; }
    else if (c >= 0xF1 && c <= 0xF3) { len = 4; }
    else if (c == 0xF4)         { len = 4; firstHi = 0x8F; }  // cap at U+10FFFF

    bool valid = (len != 0) && (i + len <= n);
    if (valid) {
      unsigned char c1 = static_cast<unsigned char>(str[i + 1]);
      if (c1 < firstLo || c1 > firstHi) valid = false;
      for (size_t k = 2; valid && k < len; k++) {
        unsigned char ck = static_cast<unsigned char>(str[i + k]);
        if (ck < 0x80 || ck > 0xBF) valid = false;
      }
    }

    if (!valid) {
      ss << kReplacement;
      i++;  // resync one byte at a time
      continue;
    }
    ss.write(str.data() + i, static_cast<std::streamsize>(len));
    i += len;
  }
  return ss.str();
}

std::optional<std::string> parseJSONString(const std::string& json, const std::string& key) {
  std::string searchKey = "\"" + key + "\"";
  size_t keyPos = json.find(searchKey);
  if (keyPos == std::string::npos) {
    return std::nullopt;
  }

  // Find colon after key
  size_t colonPos = json.find(':', keyPos + searchKey.length());
  if (colonPos == std::string::npos) {
    return std::nullopt;
  }

  // Find opening quote
  size_t quoteStart = json.find('"', colonPos + 1);
  if (quoteStart == std::string::npos) {
    return std::nullopt;
  }

  // Find closing quote (handle escapes)
  size_t i = quoteStart + 1;
  std::string result;
  while (i < json.size()) {
    if (json[i] == '\\' && i + 1 < json.size()) {
      // Handle escape sequences
      switch (json[i + 1]) {
        case '"':  result += '"'; break;
        case '\\': result += '\\'; break;
        case 'n':  result += '\n'; break;
        case 'r':  result += '\r'; break;
        case 't':  result += '\t'; break;
        default:   result += json[i + 1]; break;
      }
      i += 2;
    } else if (json[i] == '"') {
      return result;
    } else {
      result += json[i];
      i++;
    }
  }

  return std::nullopt;
}

std::optional<int> parseJSONInt(const std::string& json, const std::string& key) {
  std::string searchKey = "\"" + key + "\"";
  size_t keyPos = json.find(searchKey);
  if (keyPos == std::string::npos) {
    return std::nullopt;
  }

  // Find colon after key
  size_t colonPos = json.find(':', keyPos + searchKey.length());
  if (colonPos == std::string::npos) {
    return std::nullopt;
  }

  // Skip whitespace
  size_t numStart = colonPos + 1;
  while (numStart < json.size() && std::isspace(json[numStart])) {
    numStart++;
  }

  // Parse number
  if (numStart >= json.size()) {
    return std::nullopt;
  }

  bool negative = false;
  if (json[numStart] == '-') {
    negative = true;
    numStart++;
  }

  long long result = 0;
  while (numStart < json.size() &&
         std::isdigit(static_cast<unsigned char>(json[numStart]))) {
    result = result * 10 + (json[numStart] - '0');
    if (result > INT32_MAX) {
      return std::nullopt;  // Overflow
    }
    numStart++;
  }

  int intResult = static_cast<int>(negative ? -result : result);
  return intResult;
}

// ---- top-level-only member extraction --------------------------------------
// Minimal string-aware scanner for parseJSONStringTopLevel. Kept separate from
// parseJSONString on purpose: that one is a frozen naive substring search that
// several flat-payload call sites depend on; this one walks ONLY the top level
// of the outermost object, so nested keys (params.sessionId) and key text inside
// string values can never match.

// Decode the JSON string literal starting at json[i] == '"'. On success appends
// the decoded content to `out` and leaves `i` one past the closing quote.
static bool decodeStringAt(const std::string& json, size_t& i, std::string& out) {
  i++;  // skip opening quote
  while (i < json.size()) {
    char c = json[i];
    if (c == '\\' && i + 1 < json.size()) {
      switch (json[i + 1]) {
        case '"':  out += '"'; break;
        case '\\': out += '\\'; break;
        case 'n':  out += '\n'; break;
        case 'r':  out += '\r'; break;
        case 't':  out += '\t'; break;
        default:   out += json[i + 1]; break;
      }
      i += 2;
    } else if (c == '"') {
      i++;
      return true;
    } else {
      out += c;
      i++;
    }
  }
  return false;  // unterminated string
}

static void skipWS(const std::string& json, size_t& i) {
  while (i < json.size() && std::isspace(static_cast<unsigned char>(json[i]))) i++;
}

// Skip one JSON value starting at `i` (string / object / array / scalar),
// tracking brace/bracket depth and skipping over string contents.
static bool skipValueAt(const std::string& json, size_t& i) {
  skipWS(json, i);
  if (i >= json.size()) return false;
  char c = json[i];
  if (c == '"') {
    std::string sink;
    return decodeStringAt(json, i, sink);
  }
  if (c == '{' || c == '[') {
    int depth = 0;
    bool inString = false;
    while (i < json.size()) {
      char ch = json[i];
      if (inString) {
        if (ch == '\\' && i + 1 < json.size()) { i += 2; continue; }
        if (ch == '"') inString = false;
        i++;
      } else {
        if (ch == '"') { inString = true; i++; }
        else if (ch == '{' || ch == '[') { depth++; i++; }
        else if (ch == '}' || ch == ']') {
          depth--; i++;
          if (depth == 0) return true;
        }
        else i++;
      }
    }
    return false;  // unbalanced
  }
  // Scalar (number / true / false / null): runs until a delimiter.
  while (i < json.size() && json[i] != ',' && json[i] != '}' && json[i] != ']') i++;
  return true;
}

std::optional<std::string> parseJSONStringTopLevel(const std::string& json,
                                                   const std::string& key) {
  size_t i = json.find('{');
  if (i == std::string::npos) return std::nullopt;
  i++;  // inside the outermost object
  while (i < json.size()) {
    skipWS(json, i);
    if (i >= json.size()) return std::nullopt;
    if (json[i] == '}') return std::nullopt;  // end of top-level object
    if (json[i] == ',') { i++; continue; }
    if (json[i] != '"') return std::nullopt;  // malformed member key
    std::string k;
    if (!decodeStringAt(json, i, k)) return std::nullopt;
    skipWS(json, i);
    if (i >= json.size() || json[i] != ':') return std::nullopt;
    i++;  // skip ':'
    skipWS(json, i);
    if (k == key) {
      if (i < json.size() && json[i] == '"') {
        std::string v;
        if (decodeStringAt(json, i, v)) return v;
      }
      return std::nullopt;  // key present but value is not a string
    }
    if (!skipValueAt(json, i)) return std::nullopt;
  }
  return std::nullopt;
}

}  // namespace cdp
}  // namespace keel::devtools

#endif  // KEEL_WIP_CDP_DEVTOOLS
