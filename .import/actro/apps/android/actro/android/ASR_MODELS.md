# Android offline ASR models

The Android build downloads and verifies two official Vosk mobile models, then
adds the generated model tree to the APK. The installed app therefore retains
offline speech recognition; only a clean build without a warm Gradle cache
requires network access.

| Asset directory | Official artifact | Archive SHA-256 | License |
| --- | --- | --- | --- |
| `app/src/main/assets/model/asr/en` | [`vosk-model-small-en-us-0.15.zip`](https://alphacephei.com/vosk/models/vosk-model-small-en-us-0.15.zip) | `30f26242c4eb449f948e42cb302dd7a686cb29a3423a8367f99ff41780942498` | Apache-2.0 |
| `app/src/main/assets/model/asr/zh` | [`vosk-model-small-cn-0.22.zip`](https://alphacephei.com/vosk/models/vosk-model-small-cn-0.22.zip) | `3af8b0e7e0f835ae9d414ce5df580237a3cfb08d586c9fbbb0f7ff29ad5b14ba` | Apache-2.0 |

The former checked-in model directories were compared file-by-file with those
archives on 2026-07-29 and matched exactly. Artifact versions, URLs, archive
hashes, extraction paths, and required files are now pinned in
`binary-assets.lock.properties`.

`ModelLoader.kt` copies the selected model from APK assets to the app's
internal files directory because Vosk requires a filesystem path.

`prepareAsrModels` runs before the app's `preBuild`, verifies each archive
before extraction, and generates:

```text
app/build/generated/actro-binary-assets/asr/assets/model/asr/{en,zh}
```

Verified archives are cached under
`$GRADLE_USER_HOME/caches/actro-binary-assets`. Gradle `--offline` builds work
after that cache has been warmed and fail with an actionable error otherwise.
