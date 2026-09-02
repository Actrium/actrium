# Actr ICE Configuration

语言：[English](ice-configuration.md) | **简体中文**

状态：`actr-ice-config/1` 初始草案。

`ice-config` service relation 指向一个 HTTP endpoint，Host 从中取得该 authority 的 STUN
和 TURN 配置。公开 authority descriptor 只包含这个 endpoint，绝不直接嵌入 TURN 密码。

## 请求与响应

服务支持带 `Accept: application/json` 的 `GET`，生产客户端必须使用 HTTPS。成功响应遵循
[`ice-configuration.schema.json`](../../contracts/actr/ice-configuration.schema.json)：

```json
{
  "version": 1,
  "ice_servers": [
    {
      "urls": ["stun:rtc.example.com:3478"]
    },
    {
      "urls": ["turn:rtc.example.com:3478"],
      "username": "short-lived-username",
      "credential": "short-lived-credential"
    }
  ]
}
```

匿名响应可以公布 STUN URL 和不带 credential 的 TURN 地址。TURN 需要 credential 时，Host
必须先经过 authority 的 identity flow 完成认证。这里返回的 credential 必须短期有效，响应
也必须使用 private 且相应较短的 cache lifetime。

客户端应忽略未知字段，拒绝不支持的版本，并应用正常的 WebRTC ICE URL、transport 与本地
网络策略。服务不能仅因为调用者知道一个 `actr:` URI 就签发 credential。

Loran 当前通过该 endpoint 公布配置的 STUN/TURN 地址；与 Actor 绑定、短期有效的 TURN
credential 仍由 AIS registration 与 renewal 签发。
