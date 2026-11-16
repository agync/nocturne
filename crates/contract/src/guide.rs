//! The Nocturne guide as it ships inside the app, and what it adds to an answer. The guide is
//! never a memory: none of these types is kept, exported or backed up.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// A run of text in a guide sentence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "camelCase")]
#[ts(export)]
pub enum GuideInline {
    Text {
        text: String,
    },
    /// Words the guide stresses, or another app's control.
    Strong {
        text: String,
    },
    /// A screen or control of Nocturne, in its own words.
    Control {
        text: String,
    },
    /// A file name, a folder or a command, as typed.
    Code {
        text: String,
    },
}

/// One block of a guide section.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "camelCase")]
#[ts(export)]
pub enum GuideBlock {
    Paragraph {
        parts: Vec<GuideInline>,
    },
    /// Numbered steps, in order.
    Steps {
        items: Vec<Vec<GuideInline>>,
    },
    List {
        items: Vec<Vec<GuideInline>>,
    },
    /// The first row is the header.
    Table {
        rows: Vec<Vec<Vec<GuideInline>>>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct GuideSection {
    pub slug: String,
    pub heading: String,
    pub blocks: Vec<GuideBlock>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct GuidePage {
    pub slug: String,
    pub title: String,
    /// The page's opening paragraph.
    pub opening: Vec<GuideInline>,
    pub sections: Vec<GuideSection>,
}

/// The sections of the guide that help may use, as they came with this version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct GuideView {
    pub version: String,
    pub pages: Vec<GuidePage>,
}

/// A guide section an answer cites.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct GuideCite {
    /// `guide:{page}#{section}`, as the answer cites it.
    pub id: String,
    pub page: String,
    pub page_title: String,
    pub section: String,
    pub heading: String,
    /// The guide's own sentence for the receipt: the section's first.
    pub quote: String,
    /// The first sentence of each step, or of each paragraph when the section has no steps.
    pub steps: Vec<Vec<GuideInline>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum HelpKind {
    /// One answer written on this computer from the guide and what was kept.
    Written,
    /// The guide's own words, on a size of computer that has not passed the help check.
    Words,
    /// "Did you mean help with Nocturne?" after the normal answer.
    Unsure,
}

/// What the guide adds to an answer, on its terminal delta.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct HelpView {
    pub kind: HelpKind,
    /// The sections, in the order the answer cites them (for `unsure`, the nearest one).
    pub sections: Vec<GuideCite>,
    /// The page's opening paragraph, for the guide's own words.
    pub opening: Vec<GuideInline>,
    /// Every source a written answer cites, in order: guide sections as their `id`, memories by
    /// theirs. The nth bold span stands for the nth of them.
    pub cited: Vec<String>,
    /// Online answers was on: the mark says help is always written here.
    pub online_on: bool,
}
