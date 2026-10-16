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
              type: "actrium.host.request",
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
            if (!response || response.type !== "actrium.host.response") {
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
              getCoreStatus: () => request("actrium.host.getCoreStatus"),
              openTab: (params) => request("actrium.host.openTab", params),
              closeTab: (params) => request(
                "actrium.host.closeTab",
                typeof params === "string" ? { tabId: params } : params,
              ),
              selectTab: (tabId) => request("actrium.host.selectTab", { tabId }),
              navigate: (params) => request(
                "actrium.host.navigate",
                typeof params === "string" ? { url: params } : params,
              ),
              getTabs: () => request("actrium.host.getTabs"),
              getActiveTab: () => request("actrium.host.getActiveTab"),
              removeBookmark: (params) => request(
                "actrium.host.removeBookmark",
                typeof params === "string" ? { address: params } : params,
              ),
              removeHistoryEntry: (params) => request(
                "actrium.host.removeHistoryEntry",
                typeof params === "string" ? { address: params } : params,
              ),
            }),
          });
        })();
        """;
}
