use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Arrowhead {
    Arrow,
    Bar,
    Circle,
    CircleOutline,
    Triangle,
    TriangleOutline,
    Diamond,
    DiamondOutline,
    Dot,
    CardinalityOne,
    CardinalityMany,
    CardinalityOneOrMany,
    CardinalityExactlyOne,
    CardinalityZeroOrOne,
    CardinalityZeroOrMany,
    CrowfootOne,
    CrowfootMany,
    CrowfootOneOrMany,
}

impl Arrowhead {
    pub fn is_cardinality(self) -> bool {
        matches!(
            self,
            Self::CardinalityOne
                | Self::CardinalityMany
                | Self::CardinalityOneOrMany
                | Self::CardinalityExactlyOne
                | Self::CardinalityZeroOrOne
                | Self::CardinalityZeroOrMany
        )
    }

    pub fn is_crowfoot(self) -> bool {
        matches!(
            self,
            Self::CrowfootOne | Self::CrowfootMany | Self::CrowfootOneOrMany
        )
    }

    pub fn is_outline(self) -> bool {
        matches!(
            self,
            Self::CircleOutline | Self::TriangleOutline | Self::DiamondOutline
        )
    }
}
