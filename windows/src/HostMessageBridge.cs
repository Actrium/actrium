using Microsoft.Web.WebView2.Core;

namespace Actrium.Windows;

internal sealed class HostMessageBridge
{
    private readonly CoreWebView2 coreWebView;
    private readonly HostBridgeDispatcher dispatcher;
    private bool isAttached;

    public HostMessageBridge(
        Guid ownerTabId,
        CoreWebView2 coreWebView,
        HostBridgeActions actions,
        Func<string, bool>? isSourceAllowed = null)
    {
        this.coreWebView = coreWebView;
        dispatcher = new HostBridgeDispatcher(ownerTabId, actions, isSourceAllowed);
    }

    public async Task AttachAsync()
    {
        if (isAttached)
        {
            return;
        }

        coreWebView.WebMessageReceived += OnWebMessageReceived;
        await coreWebView.AddScriptToExecuteOnDocumentCreatedAsync(BootstrapScript);
        isAttached = true;
    }

    internal async Task<string> HandleMessageJsonAsync(string messageJson, string source = "")
    {
        return await dispatcher.HandleMessageJsonAsync(messageJson, source);
    }

    private async void OnWebMessageReceived(object? sender, CoreWebView2WebMessageReceivedEventArgs args)
    {
        var responseJson = await HandleMessageJsonAsync(args.WebMessageAsJson, args.Source);
        try
        {
            coreWebView.PostWebMessageAsJson(responseJson);
        }
        catch (ObjectDisposedException)
        {
        }
        catch (InvalidOperationException)
        {
        }
    }

    private const string BootstrapScript = """
        (() => {
          if (!window.chrome || !window.chrome.webview || window.actriumHost) {
            return;
          }

          const pending = new Map();
          let nextId = 1;

          const request = (method, params) => {
            const id = `${Date.now()}-${nextId++}`;
            const message = {
              type: "actro.host.request",
              id,
              method,
              params: params ?? null,
            };

            return new Promise((resolve, reject) => {
              pending.set(id, { resolve, reject });
              window.chrome.webview.postMessage(message);
            });
          };

          window.chrome.webview.addEventListener("message", (event) => {
            const response = event.data;
            if (!response || response.type !== "actro.host.response") {
              return;
            }

            const callback = pending.get(response.id);
            if (!callback) {
              return;
            }

            pending.delete(response.id);
            if (response.ok) {
              callback.resolve(response.result);
            } else {
              callback.reject(response.error);
            }
          });

          Object.defineProperty(window, "actriumHost", {
            configurable: false,
            enumerable: true,
            writable: false,
            value: Object.freeze({
              protocolVersion: 1,
              getCoreStatus: () => request("actro.host.getCoreStatus"),
              openTab: (params) => request("actro.host.openTab", params),
              closeTab: (params) => request(
                "actro.host.closeTab",
                typeof params === "string" ? { tabId: params } : params,
              ),
              selectTab: (tabId) => request("actro.host.selectTab", { tabId }),
              navigate: (params) => request(
                "actro.host.navigate",
                typeof params === "string" ? { url: params } : params,
              ),
              getTabs: () => request("actro.host.getTabs"),
              getActiveTab: () => request("actro.host.getActiveTab"),
              removeBookmark: (params) => request(
                "actro.host.removeBookmark",
                typeof params === "string" ? { address: params } : params,
              ),
              removeHistoryEntry: (params) => request(
                "actro.host.removeHistoryEntry",
                typeof params === "string" ? { address: params } : params,
              ),
            }),
          });
        })();
        """;
}
