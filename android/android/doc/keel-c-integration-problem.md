# Keel C Code Integration and TurboModuleRegistry Mistakes

---

## 1. Problem

Adoption of the item ActriumRuntime It's... React Native The modules are indirectly integrated in a local-dependent manner keel Project.keel Include C Language code, but in the current project process,keel It's... C The code was not properly compiled into the final product.

To solve the problem, I tried to find out app/gradle Add in Configuration keel Library C path to the code, but the following error occurred during the run:

```
com.facebook.react.common.JavascriptException: TurboModuleRegistry.getEnforcing(...): 'PlatformConstants' could not be found. Verify that a module by this name is registered in the native binary., stack:
Invariant Violation: TurboModuleRegistry.getEnforcing(...): 'PlatformConstants' could not be found. Verify that a module by this name is registered in the native binary.
```

Performance of the issue and [React Native Official issue #47352](https://github.com/facebook/react-native/issues/47352) Exactly.

---

## 2. Problem analysis

The error in reporting occurred React Native Run through `TurboModuleRegistry.getEnforcing(...)` Fetch Name `PlatformConstants` It's... TurboModule Failure.

`PlatformConstants` Yes React Native Core module.“Core module not found”Not because of the lack of a custom module for business code, but because of the actual loading of the current application native binary and their initialization, registration processes and React Native Unsatisfactory expectations, resulting in TurboModule Failure to properly register or incomplete registration form.

Integration of the project (for integration) keel It's... C Codes and adjust app/gradle It's... native Build configuration) and it is clearly established that the root cause of the problem is the use of customised project CMake File without applying default CMake Configure. This change leads to native binary Change in the grouping approach, and thus impact React Native The registration and loading process for the core module.

---

## 3. Solutions

There are two paths to repair this problem:

### Option I: Upgrade React Native Version

The question is one of React Native The script is not prepared. [PR #47379](https://github.com/facebook/react-native/pull/47379/changes) It's been repaired. In theory, it's possible to upgrade. React Native The issue is to be completely resolved by including the version of the restoration.

But limited to current projects and related dependency React Native The compatibility requirement of the version is not yet in place for the time being.

### Programme II: pre-compilation keel C Code is .so File and Insert jniLibs(current programme)

The current programme is: keel Projects C Code Compile As .so File, then place it in jniLibs Contents.

No changes are required to the programme app It's... externalNativeBuild or CMake Configure, effectively circumvent React Native Loss of core module to ensure native binary The composition and registration process are consistent React Native Expected.

---

## 4. Emerging issues

Option II may temporarily address integration issues, but whenever keel C When the code changes, the corresponding code must be regenerated and replaced .so File. The process relies on manual operations, which can lead to construction not being consistent with source code (e.g. missing updates). .so Or misuse old version .so) and cannot achieve an instantiality of the source level, affecting development efficiency and maintenance.
