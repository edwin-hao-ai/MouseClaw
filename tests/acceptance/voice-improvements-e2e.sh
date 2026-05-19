#!/usr/bin/env bash
# Acceptance: v0.4.0 voice improvements (P0 双语锁死 / P1 sherpa hotwords 术语库 /
#             P2 3 秒规则纠错 / drag-detector 全屏迎接拖拽).
# Structural grep guard — catches future PRs breaking the contract.
# Run: bash tests/acceptance/voice-improvements-e2e.sh
set -uo pipefail
cd "$(dirname "$0")/../.."

PASS=0; FAIL=0
ok()   { echo "  ✅ $1"; PASS=$((PASS+1)); }
no()   { echo "  ❌ $1"; FAIL=$((FAIL+1)); }
check(){ if eval "$2" >/dev/null 2>&1; then ok "$1"; else no "$1"; fi; }

echo "── P0 · 中英双语模型锁死 ──"
check "transcribe_stream 锁死 zh-en"               'grep -q "active_lang.*zh-en" src-tauri/src/transcribe_stream.rs'
check "EN_MODEL_DIR/FILES 已删除"                  '! grep -q "EN_MODEL_DIR" src-tauri/src/transcribe_stream.rs'
check "default_voice_lang 是 zh-en"                'grep -q "default_voice_lang.*zh-en" src-tauri/src/config.rs'
check "save_shortcut 忽略 voice_lang 值"           'grep -q "voice_lang = .*zh-en" src-tauri/src/commands.rs'
check "Onboarding VoiceLang 单值类型"              'grep -q "VoiceLang = .zh-en" src/components/Onboarding.tsx'
check "Onboarding 删除二选一 radio"                '! grep -q "voiceLang === .zh" src/components/Onboarding.tsx'
check "i18n 新 key · voice_setup.mixed_label"      'grep -q "voice_setup.mixed_label" src/i18n/zh.ts && grep -q "voice_setup.mixed_label" src/i18n/en.ts && grep -q "voice_setup.mixed_label" src/i18n/types.ts'

echo "── P1 · sherpa hotwords 术语库 ──"
check "vocab.rs 存在"                              'test -f src-tauri/src/vocab.rs'
check "lib.rs 注册 vocab 模块"                     'grep -q "pub mod vocab" src-tauri/src/lib.rs'
check "vocab::BUILTIN_PROGRAMMER include_str!"     'grep -q "include_str!.*resources/vocab/programmer.txt" src-tauri/src/vocab.rs'
check "resources/vocab/programmer.txt 存在"        'test -f src-tauri/resources/vocab/programmer.txt'
check "内置词表 ≥40 条"                            'grep -cE "^[A-Za-z]" src-tauri/resources/vocab/programmer.txt | awk "{exit (\$1 < 40)}"'
check "config.vocab_builtin_enabled 字段"          'grep -q "vocab_builtin_enabled" src-tauri/src/config.rs'
check "transcribe_stream 注入 hotwords_file"       'grep -q "hotwords_file" src-tauri/src/transcribe_stream.rs'
check "transcribe_stream 用 modified_beam_search"  'grep -q "modified_beam_search" src-tauri/src/transcribe_stream.rs'
check "invalidate_recognizer 入口存在"             'grep -q "pub fn invalidate_recognizer" src-tauri/src/transcribe_stream.rs'
check "vocab_* 4 commands 注册"                    'grep -q "vocab_open_user_file" src-tauri/src/lib.rs && grep -q "vocab_reload" src-tauri/src/lib.rs && grep -q "vocab_set_builtin_enabled" src-tauri/src/lib.rs'
check "tray 有「📝 术语表」子菜单"                  'grep -q "vocab-submenu" src-tauri/src/tray.rs'
check "tray handler · vocab-edit/reload/toggle"    'grep -q "vocab-edit" src-tauri/src/tray.rs && grep -q "vocab-reload" src-tauri/src/tray.rs && grep -q "toggle-vocab-builtin" src-tauri/src/tray.rs'
check "startup 调 ensure_user_file + regen"        'grep -q "vocab::ensure_user_file" src-tauri/src/lib.rs && grep -q "vocab::regenerate_active" src-tauri/src/lib.rs'

echo "── P2 · 3 秒规则版语音纠错 ──"
check "voice_correct.rs 存在"                      'test -f src-tauri/src/voice_correct.rs'
check "lib.rs 注册 voice_correct 模块"             'grep -q "pub mod voice_correct" src-tauri/src/lib.rs'
check "CORRECTION_WINDOW = 3 秒"                   'grep -q "Duration::from_secs(3)" src-tauri/src/voice_correct.rs'
check "is_terminal_bundle 黑名单"                  'grep -q "com.apple.Terminal" src-tauri/src/voice_correct.rs && grep -q "dev.warp" src-tauri/src/voice_correct.rs'
check "CorrectionAction 3 种 variant"              'grep -q "Replace " src-tauri/src/voice_correct.rs && grep -q "UndoAll" src-tauri/src/voice_correct.rs && grep -q "NotFound" src-tauri/src/voice_correct.rs'
check "parse_replace 支持中文"                     'grep -q "改成" src-tauri/src/voice_correct.rs && grep -q "换为" src-tauri/src/voice_correct.rs'
check "parse_replace 支持英文"                     'grep -q "change " src-tauri/src/voice_correct.rs && grep -q "replace " src-tauri/src/voice_correct.rs'
check "is_undo_all 支持「重说/重来/算了」"          'grep -q "重说" src-tauri/src/voice_correct.rs && grep -q "scratch that" src-tauri/src/voice_correct.rs'
check "voice_ime 调 try_parse 进 handle_correction" 'grep -q "voice_correct::try_parse" src-tauri/src/voice_ime.rs && grep -q "handle_correction" src-tauri/src/voice_ime.rs'
check "voice_ime 写入后 record_write"              'grep -q "voice_correct::record_write" src-tauri/src/voice_ime.rs'
check "Replace 用 mode_b::delete_chars"            'grep -q "mode_b::delete_chars" src-tauri/src/voice_ime.rs'

echo "── 修复 · set_mode 走主线程（修按 fn 不移动到光标的回归） ──"
check "set_mode 用 run_on_main_thread"             'grep -q "app.run_on_main_thread" src-tauri/src/overlay_size.rs'

echo "── drag-detector · 桌宠跑去迎接拖拽 ──"
check "drag_detector.rs 存在"                      'test -f src-tauri/src/drag_detector.rs'
check "lib.rs 注册 drag_detector 模块"             'grep -q "pub mod drag_detector" src-tauri/src/lib.rs'
check "lib.rs setup 调 install()"                  'grep -q "drag_detector::install" src-tauri/src/lib.rs'
check "MCDragView NSView 子类"                     'grep -q "MCDragView" src-tauri/src/drag_detector.rs'
check "NSDragging 4 个 protocol 方法"              'grep -q "draggingEntered:" src-tauri/src/drag_detector.rs && grep -q "draggingUpdated:" src-tauri/src/drag_detector.rs && grep -q "draggingExited:" src-tauri/src/drag_detector.rs && grep -q "performDragOperation:" src-tauri/src/drag_detector.rs'
check "setIgnoresMouseEvents:YES (100% click-through)" 'grep -q "setIgnoresMouseEvents:YES" src-tauri/src/drag_detector.rs'
check "只接 public.file-url (不抢文本拖动)"        'grep -q "public.file-url" src-tauri/src/drag_detector.rs'
check "collectionBehavior 全屏 / 跨 Space"         'grep -q "setCollectionBehavior" src-tauri/src/drag_detector.rs'
check "转发到 feed_flow::on_drag_enter/leave/dropped" 'grep -q "feed_flow::on_drag_enter" src-tauri/src/drag_detector.rs && grep -q "feed_flow::on_drag_leave" src-tauri/src/drag_detector.rs && grep -q "feed_flow::on_files_dropped" src-tauri/src/drag_detector.rs'

echo "── 单元测试金字塔（cargo test） ──"
if cargo test --manifest-path src-tauri/Cargo.toml -p mouseclaw --lib "vocab::tests" --quiet 2>/dev/null | grep -q "test result: ok"; then
    ok "vocab unit tests pass"
else
    no "vocab unit tests FAIL"
fi
if cargo test --manifest-path src-tauri/Cargo.toml -p mouseclaw --lib "voice_correct::tests" --quiet -- --test-threads=1 2>/dev/null | grep -q "test result: ok"; then
    ok "voice_correct unit tests pass"
else
    no "voice_correct unit tests FAIL"
fi

echo "── 类型 / 编译 ──"
if bunx tsc --noEmit 2>&1 | grep -q error; then
    no "tsc 编译 (frontend) failed"
else
    ok "tsc 编译 (frontend) clean"
fi
if cargo check --manifest-path src-tauri/Cargo.toml 2>&1 | grep -q "^error\["; then
    no "cargo check (rust) failed"
else
    ok "cargo check (rust) clean"
fi

echo
echo "─────────────────────────────────────"
echo " PASS=$PASS · FAIL=$FAIL"
echo "─────────────────────────────────────"
if [ "$FAIL" -gt 0 ]; then exit 1; fi
