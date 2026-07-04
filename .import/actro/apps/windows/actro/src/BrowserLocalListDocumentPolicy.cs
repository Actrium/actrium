using System.Net;
using System.Text.Json;

namespace Actro.Windows;

internal static class BrowserLocalListDocumentPolicy
{
    public static string BuildDocument(BrowserLocalListDocumentOptions options)
    {
        var safeTitle = WebUtility.HtmlEncode(options.Title);
        var emptyMessageHtml = $"<p>{WebUtility.HtmlEncode(options.EmptyMessage)}</p>";
        var search = options.Items.Count == 0
            ? string.Empty
            : BuildSearchInput(options.SearchLabel, options.SearchPlaceholder);
        var body = options.Items.Count == 0
            ? emptyMessageHtml
            : BuildList(options);
        var safeEmptyFilterMessage = WebUtility.HtmlEncode(options.EmptyFilterMessage);
        var emptyMessageJson = JsonSerializer.Serialize(emptyMessageHtml);
        var removeAttributeJson = JsonSerializer.Serialize(options.RemoveAttributeName);
        var removeHostMethodJson = JsonSerializer.Serialize(options.RemoveHostMethodName);

        return $$"""
        <!doctype html>
        <html>
          <head>
            <meta charset="utf-8">
            <title>{{safeTitle}}</title>
            <style>
              html, body {
                margin: 0;
                font-family: "Segoe UI", system-ui, sans-serif;
                background: #f6f7f9;
                color: #121417;
              }
              main {
                box-sizing: border-box;
                max-width: 920px;
                margin: 0 auto;
                padding: 48px 32px;
              }
              h1 {
                font-size: 28px;
                font-weight: 600;
                margin: 0 0 24px;
              }
              p {
                color: #59616d;
                font-size: 15px;
                margin: 0;
              }
              .list-search {
                background: #ffffff;
                border: 1px solid #c8d0dc;
                border-radius: 4px;
                box-sizing: border-box;
                color: #121417;
                font: 15px "Segoe UI", system-ui, sans-serif;
                margin: 0 0 16px;
                padding: 9px 11px;
                width: 100%;
              }
              .list-search:focus {
                border-color: #165dff;
                outline: 2px solid rgba(22, 93, 255, 0.16);
              }
              ol {
                display: grid;
                gap: 12px;
                list-style: none;
                margin: 0;
                padding: 0;
              }
              li {
                background: #ffffff;
                border: 1px solid #dfe3ea;
                border-radius: 6px;
                display: flex;
                gap: 12px;
                padding: 14px 16px;
              }
              .item-content {
                flex: 1;
                min-width: 0;
              }
              a {
                color: #165dff;
                display: block;
                font-size: 15px;
                font-weight: 600;
                margin-bottom: 6px;
                overflow-wrap: anywhere;
                text-decoration: none;
              }
              a:hover {
                text-decoration: underline;
              }
              code {
                color: #59616d;
                display: block;
                font: 12px "Cascadia Mono", "Consolas", monospace;
                margin-bottom: 6px;
                overflow-wrap: anywhere;
              }
              time {
                color: #7a8494;
                font-size: 12px;
              }
              button {
                align-self: flex-start;
                background: #ffffff;
                border: 1px solid #c8d0dc;
                border-radius: 4px;
                color: #374151;
                cursor: pointer;
                font: 13px "Segoe UI", system-ui, sans-serif;
                padding: 5px 10px;
              }
              button:hover {
                background: #f1f4f8;
              }
              button:disabled {
                color: #9aa3af;
                cursor: default;
              }
            </style>
          </head>
          <body>
            <main>
              <h1>{{safeTitle}}</h1>
              {{search}}
              {{body}}
              <p data-empty-filter hidden>{{safeEmptyFilterMessage}}</p>
            </main>
            <script>
              (() => {
                const emptyMessage = {{emptyMessageJson}};
                const removeAttribute = {{removeAttributeJson}};
                const removeHostMethod = {{removeHostMethodJson}};
                const searchInput = document.querySelector("[data-list-search]");
                const emptyFilterMessage = document.querySelector("[data-empty-filter]");
                const getRows = () => Array.from(document.querySelectorAll("li[data-filter]"));

                const applyFilter = () => {
                  const query = searchInput ? searchInput.value.trim().toLowerCase() : "";
                  let visibleCount = 0;
                  for (const row of getRows()) {
                    const text = (row.getAttribute("data-filter") || "").toLowerCase();
                    const isVisible = !query || text.includes(query);
                    row.hidden = !isVisible;
                    if (isVisible) {
                      visibleCount += 1;
                    }
                  }

                  if (emptyFilterMessage) {
                    emptyFilterMessage.hidden = visibleCount !== 0;
                  }
                };

                if (searchInput) {
                  searchInput.addEventListener("input", applyFilter);
                }

                document.addEventListener("click", async (event) => {
                  const target = event.target instanceof Element ? event.target : null;
                  const button = target ? target.closest(`button[${removeAttribute}]`) : null;
                  const hostAction = window.actroHost ? window.actroHost[removeHostMethod] : null;
                  if (!button || typeof hostAction !== "function") {
                    return;
                  }

                  const address = button.getAttribute(removeAttribute);
                  if (!address) {
                    return;
                  }

                  button.disabled = true;
                  try {
                    const removed = await hostAction({ address });
                    if (removed) {
                      const row = button.closest("li");
                      const list = row ? row.parentElement : null;
                      if (row) {
                        row.remove();
                      }

                      if (list && list.children.length === 0) {
                        list.outerHTML = emptyMessage;
                        if (searchInput) {
                          searchInput.remove();
                        }

                        if (emptyFilterMessage) {
                          emptyFilterMessage.remove();
                        }
                      } else {
                        applyFilter();
                      }

                      return;
                    }
                  } catch {
                  }

                  button.disabled = false;
                });
              })();
            </script>
          </body>
        </html>
        """;
    }

    private static string BuildSearchInput(string label, string placeholder)
    {
        return
            $"<input class=\"list-search\" data-list-search type=\"search\" aria-label=\"{WebUtility.HtmlEncode(label)}\" placeholder=\"{WebUtility.HtmlEncode(placeholder)}\">";
    }

    private static string BuildList(BrowserLocalListDocumentOptions options)
    {
        return $"<ol>{string.Concat(options.Items.Select(item => BuildItem(item, options.RemoveAttributeName)))}</ol>";
    }

    private static string BuildItem(BrowserLocalListDocumentItem item, string removeAttributeName)
    {
        var safeTitle = WebUtility.HtmlEncode(item.Title);
        var safeAddress = WebUtility.HtmlEncode(item.Address);
        var safeDetail = WebUtility.HtmlEncode(item.Detail);
        var safeFilter = WebUtility.HtmlEncode(item.FilterText);

        return $$"""
        <li data-filter="{{safeFilter}}">
          <div class="item-content">
            <a href="{{safeAddress}}">{{safeTitle}}</a>
            <code>{{safeAddress}}</code>
            <time>{{safeDetail}}</time>
          </div>
          <button type="button" {{removeAttributeName}}="{{safeAddress}}">Remove</button>
        </li>
        """;
    }
}

internal sealed record BrowserLocalListDocumentOptions(
    string Title,
    string EmptyMessage,
    string SearchLabel,
    string SearchPlaceholder,
    string EmptyFilterMessage,
    string RemoveAttributeName,
    string RemoveHostMethodName,
    IReadOnlyList<BrowserLocalListDocumentItem> Items);

internal sealed record BrowserLocalListDocumentItem(
    string Title,
    string Address,
    string Detail,
    string FilterText);
