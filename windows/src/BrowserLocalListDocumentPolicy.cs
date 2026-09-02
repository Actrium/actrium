using System.Net;
using System.Text.Json;

namespace Actrium.Windows;

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
              {{BrowserDocumentStylePolicy.RootCss}}
              html, body {
                margin: 0;
                font-family: var(--ui-font);
                background: var(--page-background);
                color: var(--page-foreground);
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
                color: var(--muted-foreground);
                font-size: 15px;
                margin: 0;
              }
              .list-search {
                background: var(--input-background);
                border: 1px solid var(--border);
                border-radius: 4px;
                box-sizing: border-box;
                color: var(--page-foreground);
                font: 15px var(--ui-font);
                margin: 0 0 16px;
                padding: 9px 11px;
                width: 100%;
              }
              .list-search:focus {
                border-color: var(--accent);
                outline: 2px solid var(--accent-soft);
              }
              ol {
                display: grid;
                gap: 12px;
                list-style: none;
                margin: 0;
                padding: 0;
              }
              li {
                background: var(--card-background);
                border: 1px solid var(--border);
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
                color: var(--accent);
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
                color: var(--muted-foreground);
                display: block;
                font: 12px var(--mono-font);
                margin-bottom: 6px;
                overflow-wrap: anywhere;
              }
              time {
                color: var(--disabled-foreground);
                font-size: 12px;
              }
              button {
                align-self: flex-start;
                background: var(--card-background);
                border: 1px solid var(--border);
                border-radius: 4px;
                color: var(--page-foreground);
                cursor: pointer;
                font: 13px var(--ui-font);
                padding: 5px 10px;
              }
              button:hover {
                background: var(--hover-background);
              }
              button:disabled {
                color: var(--disabled-foreground);
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
                  const hostAction = window.actriumHost ? window.actriumHost[removeHostMethod] : null;
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
