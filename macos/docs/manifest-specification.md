# Actro ActrApp Manifest Normative

Version:1.0.0
Last update:2025-12-07

## General

Actro ActrApp Use `manifest.json` File describes the metadata and layouts that apply. This is referenced. Chrome Extension Manifest V3 The design concept, using a declarative configuration.

## Basic structure

```json
{
  "name": "Apply Name",
  "version": "Version Number",
  "description": "Apply Description",
  "author": "Author Information",
  "layout": {
    "leftPanel": "menu.js",
    "rightPanel": "ext.js",
    "rightPanelDefaultVisible": false
  }
}
```

## Field description

### Required Fields

#### `name`
- **Type**: `string`
- **shall fill**: Yes.
- **Annotations**: Only name of the identifier to be applied, recommended PascalCase Naming Code
- **Limits**:
  - Length:1-100 Character
  - Format: Letters, numbers, underline, must begin with letters
- **Example:**: `"HelloWorld"`, `"IssueManager"`

#### `version`
- **Type**: `string`
- **shall fill**: Yes.
- **Annotations**: Apply a version number, follow semantic version specifications (Semantic Versioning)
- **Format**: `MAJOR.MINOR.PATCH`
- **Example:**: `"1.0.0"`, `"2.3.1"`

#### `description`
- **Type**: `string`
- **shall fill**: Yes.
- **Annotations**: A short description of the application, indicating the main function and use of the application
- **Limits**:
  - Length:10-500 Character
  - Suggested sentence describing the core functions to be applied
- **Example:**:
  - `"A simple one. HelloWorld Apply examples, including interactive buttons"`
  - `"Item Issue Manage applications to support multi-project switching Issue Track"`

### Optional Fields

#### `author`
- **Type**: `string`
- **shall fill**: Yes
- **Annotations**: Apply author or team name
- **Example:**: `"Actro Team"`, `"Zhang San"`

#### `layout`
- **Type**: `object`
- **shall fill**: No (but recommended configuration)
- **Annotations**: Apply three-bar layouts configuration object
- **Subfields**: See below"Layout Configuration"Chapter

## Layout Configuration (Layout)

Actro ActrApp Adopt Fixed-flex-Fixed three-column layout structure:

```
┌────────────┬──────────────────┬────────────┐
│  Left Menu  │    Interlocutory dialogue window  │  Right Extension  │
│  240px     │    flex: 1       │  400px     │
│  Fixed width  │    Self-adaptation        │  Fixed width  │
└────────────┴──────────────────┴────────────┘
```

### `layout.leftPanel`
- **Type**: `string`
- **shall fill**: Yes
- **Annotations**: Footbook path for left menu (relative to applied root directory)
- **Default value**: None (left panel not shown)
- **Documentation requirements**:
  - It must be. `.js` Documentation
  - Must Export Matches Keel Plugin Normative React Component
  - Component will be 240px Rendering in a fixed container with width
- **Example:**: `"menu.js"`, `"panels/left.js"`

### `layout.rightPanel`
- **Type**: `string`
- **shall fill**: Yes
- **Annotations**: The right extension of the script file path (relative to the application root directory) for the extended panel
- **Default value**: None (right panel not shown)
- **Documentation requirements**:
  - It must be. `.js` Documentation
  - Must Export Matches Keel Plugin Normative React Component
  - Component will be 400px Rendering in a fixed container with width
- **Example:**: `"ext.js"`, `"panels/right.js"`

### `layout.rightPanelDefaultVisible`
- **Type**: `boolean`
- **shall fill**: Yes
- **Annotations**: Initial Visibility Status of Right Extension Panel
- **Default value**: `false`(Default hidden)
- **Behaviour**:
  - `true`: Apply the right panel default display on startup
  - `false`: Apply right panel default hidden on startup
  - Users can display it through the switch button of the interdialogue window/Hide
- **Example:**: `true`, `false`

## Full Example

### Example: 1: HelloWorld Apply (right panel default)

```json
{
  "name": "HelloWorld",
  "version": "1.0.0",
  "description": "A simple one. HelloWorld Apply examples, including interactive buttons",
  "author": "Actro Team",
  "layout": {
    "leftPanel": "menu.js",
    "rightPanel": "ext.js",
    "rightPanelDefaultVisible": true
  }
}
```

**Effects**: Apply the three columns to show all of them at startup, and the user can see the full interface layout.

### Example: 2: IssueManager Apply (right panel default hidden)

```json
{
  "name": "IssueManager",
  "version": "1.0.0",
  "description": "Item Issue Manage applications to support multi-project switching Issue Track",
  "author": "Actro Team",
  "layout": {
    "leftPanel": "menu.js",
    "rightPanel": "ext.js",
    "rightPanelDefaultVisible": false
  }
}
```

**Effects**: Apply to start only the left menu and the middle dialogue window, the right panel is hidden and the user can click the button to display.

### Example: 3: Minimum Configuration (dialogue window only)

```json
{
  "name": "SimpleChat",
  "version": "1.0.0",
  "description": "Exact dialogue application, sidebar"
}
```

**Effects**: Apply to start with only an intermediate dialogue window without a left-side panel.

## Validation rules

Actro Host Application Loading `.actr` The following verifications are performed:

1. **File Existence**: `manifest.json` File must be in the application root directory
2. **JSON Format**: It has to be legal. JSON Format, not allowing comments or tailing comma
3. **Required Fields**: `name`, `version`, `description` Must exist and not be empty
4. **Version Format**: `version` It must be. `x.y.z` Formatx, y, z (integrator)
5. **Script File**: `layout.leftPanel` and `layout.rightPanel` The specified file must exist

## with Chrome Extension Manifest Comparison

| Feature | Chrome Extension | Actro ActrApp |
|------|------------------|----------------|
| **Configure Filename** | `manifest.json` | `manifest.json` |
| **Version Management** | `manifest_version`, `version` | `version` |
| **Application of identifiers** | `name`, `short_name` | `name` |
| **Synchronising folder** | `description`, `author` | `description`, `author` |
| **Layout Configuration** | `browser_action`, `page_action` | `layout` (Three right-central left) |
| **Permission System** | `permissions`, `host_permissions` | Not supported for the time being (Future planning) |
| **Backstage services** | `background`, `service_worker` | Not supported for the time being (Future planning) |

**Design philosophy**:
- ✅ **Declaration Configuration**: Use JSON Declaration of application structure, not command code
- ✅ **Minimal Fill**: Only 3 A mandatory field to simplify the developer ' s burden
- ✅ **Semantic Version**: Use standard version numbering
- ✅ **Extension**: Advanced features such as privileges, life cycle hooks can be added in the future

## Future planning

The following characteristics are being planned and are not being realized:

### 1. Permission System
```json
{
  "permissions": [
    "storage",
    "network",
    "filesystem"
  ]
}
```

### 2. Life cycle hooks
```json
{
  "hooks": {
    "onInstall": "scripts/install.js",
    "onUpdate": "scripts/update.js"
  }
}
```

### 3. Content security policy (CSP)
```json
{
  "content_security_policy": {
    "extension_pages": "script-src 'self'; object-src 'self'"
  }
}
```

### 4. Internationalized support
```json
{
  "default_locale": "zh_CN",
  "locales": {
    "en": {
      "description": "A simple HelloWorld application"
    },
    "zh_CN": {
      "description": "A simple one. HelloWorld Apply"
    }
  }
}
```

## Migration guide

### From Chrome Extension Migration

If you do, Chrome Extension Experience is developed, which allows for migration according to the following mapping relationships:

| Chrome Extension | Actro ActrApp | Annotations |
|------------------|----------------|------|
| `manifest_version` | Cannot initialise Evolution's mail component. | Actro Automatically recognize version |
| `name` | `name` | Copy Directly |
| `version` | `version` | Copy Directly |
| `description` | `description` | Copy Directly |
| `author` | `author` | Copy Directly |
| `browser_action.default_popup` | `layout.leftPanel` / `layout.rightPanel` | Select according to layout requirements |
| `background.service_worker` | Not supported for the time being | Future edition will support |
| `permissions` | Not supported for the time being | Future edition will support |

## Common problems

### Q1: If Not Configure `layout` What happens to the fields?
**A**: Apply only the middle dialogue window, and the left and right panel will not be rendered.

### Q2: Dynamically modified `rightPanelDefaultVisible` - Yeah?
**A**: Not. This field only affects the initial state at startup, the display at running time/Hide is controlled by the user by the switch button.

### Q3: `leftPanel` and `rightPanel` Can you point to the same file?
**A**: You can, but not recommend. The two panels will be independently case-based components, which may lead to a non-sync situation.

### Q4: Support embedded paths (e.g., `panels/menu/index.js`)?
**A**: Supported. Paths can be used relative to the applied root directory `/` Separator specifies a subdirectories.

### Q5: Script Files Available TypeScript - Yeah?
**A**: Can not open message JavaScript File. It is suggested that you use a build tool (e.g., Babel, Webpack) Pre-edited TypeScript.

## References

- [Chrome Extension Manifest V3 Document](https://developer.chrome.com/docs/extensions/mv3/manifest/)
- [Semantic Version Regulation (Semantic Versioning)](https://semver.org/)
- [Actro ActrApp Plugin development guide](./plugin-development.md)
- [Keel Plugin Normative](./keel-plugin-spec.md)

## Version History

| Version | Date | Change |
|------|------|----------|
| 1.0.0 | 2025-12-07 | Initial version, define base field and layout configuration |
