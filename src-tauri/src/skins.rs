//! Mouse skin identifiers — Rust side mirror of src/skins.ts.
//!
//! 真正的视觉数据（调色板、体型）只在前端定义（TS 是单一信源），Rust 这边只
//! 持有皮肤 id（用来存 config + 通过事件广播给前端）。所有视觉细节由
//! PixelMouse.tsx 按 SkinId 自行解析。

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SkinId {
    Classic,
    Lab,
    Field,
    Ninja,
    Cyber,
    Golden,
    // v0.1.26 · 非啮齿目（猫 / 狐 / 蛙）—— picker 里独立分组
    CatGray,
    FoxRed,
    FrogTree,
}

impl Default for SkinId {
    fn default() -> Self {
        SkinId::Classic
    }
}

impl SkinId {
    /// 中文展示名（托盘菜单 / Onboarding 用）。
    pub fn display_name(&self) -> &'static str {
        match self {
            SkinId::Classic => "🐭 经典灰",
            SkinId::Lab => "🤍 小白鼠",
            SkinId::Field => "🌾 田鼠",
            SkinId::Ninja => "🥷 忍者鼠",
            SkinId::Cyber => "🤖 机械鼠",
            SkinId::Golden => "✨ 金鼠",
            SkinId::CatGray => "🐱 小灰猫",
            SkinId::FoxRed => "🦊 赤狐",
            SkinId::FrogTree => "🐸 树蛙",
        }
    }

    /// 全部 9 款，按 picker 显示顺序（先鼠后其他物种）。
    pub fn all() -> &'static [SkinId] {
        &[
            SkinId::Classic,
            SkinId::Lab,
            SkinId::Field,
            SkinId::Ninja,
            SkinId::Cyber,
            SkinId::Golden,
            SkinId::CatGray,
            SkinId::FoxRed,
            SkinId::FrogTree,
        ]
    }

    /// kebab-case id，前后端约定（与 serde 的 rename_all 等价）。
    pub fn as_str(&self) -> &'static str {
        match self {
            SkinId::Classic => "classic",
            SkinId::Lab => "lab",
            SkinId::Field => "field",
            SkinId::Ninja => "ninja",
            SkinId::Cyber => "cyber",
            SkinId::Golden => "golden",
            SkinId::CatGray => "cat-gray",
            SkinId::FoxRed => "fox-red",
            SkinId::FrogTree => "frog-tree",
        }
    }

    /// 从前端 / config 字符串解析；未知值兜底 Classic。
    pub fn from_str(s: &str) -> SkinId {
        match s {
            "lab" => SkinId::Lab,
            "field" => SkinId::Field,
            "ninja" => SkinId::Ninja,
            "cyber" => SkinId::Cyber,
            "golden" => SkinId::Golden,
            "cat-gray" => SkinId::CatGray,
            "fox-red" => SkinId::FoxRed,
            "frog-tree" => SkinId::FrogTree,
            _ => SkinId::Classic,
        }
    }

    /// 托盘子菜单 item id，例如 "skin:lab"。
    pub fn tray_menu_id(&self) -> String {
        format!("skin:{}", self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_skins_round_trip_through_str() {
        for s in SkinId::all() {
            let parsed = SkinId::from_str(s.as_str());
            assert_eq!(*s, parsed, "{:?} 必须能 round-trip", s);
        }
    }

    #[test]
    fn unknown_str_falls_back_to_classic() {
        assert_eq!(SkinId::from_str(""), SkinId::Classic);
        assert_eq!(SkinId::from_str("nonsense"), SkinId::Classic);
    }

    #[test]
    fn default_is_classic() {
        assert_eq!(SkinId::default(), SkinId::Classic);
    }

    #[test]
    fn serde_uses_kebab_case() {
        let json = serde_json::to_string(&SkinId::Cyber).unwrap();
        assert_eq!(json, "\"cyber\"");
        let back: SkinId = serde_json::from_str("\"golden\"").unwrap();
        assert_eq!(back, SkinId::Golden);
    }

    #[test]
    fn tray_menu_id_is_namespaced() {
        assert_eq!(SkinId::Lab.tray_menu_id(), "skin:lab");
    }
}
