//! Item categories and category groups.
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// One table gives the enum, `as_str` and `from_name`, so they can't drift apart.
macro_rules! item_categories {
    ($($variant:ident => $name:literal,)*) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub enum ItemCategory {
            $($variant,)*
        }

        impl ItemCategory {
            pub const fn as_str(self) -> &'static str {
                match self {
                    $(ItemCategory::$variant => $name,)*
                }
            }

            pub fn from_name(name: &str) -> Option<Self> {
                match name {
                    $($name => Some(ItemCategory::$variant),)*
                    _ => None,
                }
            }
        }
    };
}

item_categories! {
    Map => "Map",
    CapturedBeast => "Captured Beast",
    MetamorphSample => "Metamorph Sample",
    Helmet => "Helmet",
    BodyArmour => "Body Armour",
    Gloves => "Gloves",
    Boots => "Boots",
    Shield => "Shield",
    Amulet => "Amulet",
    Belt => "Belt",
    Ring => "Ring",
    Flask => "Flask",
    AbyssJewel => "Abyss Jewel",
    Jewel => "Jewel",
    Quiver => "Quiver",
    Claw => "Claw",
    Bow => "Bow",
    Sceptre => "Sceptre",
    Wand => "Wand",
    FishingRod => "Fishing Rod",
    Staff => "Staff",
    Warstaff => "Warstaff",
    Dagger => "Dagger",
    RuneDagger => "Rune Dagger",
    OneHandedAxe => "One-Handed Axe",
    TwoHandedAxe => "Two-Handed Axe",
    OneHandedMace => "One-Handed Mace",
    TwoHandedMace => "Two-Handed Mace",
    OneHandedSword => "One-Handed Sword",
    TwoHandedSword => "Two-Handed Sword",
    ClusterJewel => "Cluster Jewel",
    HeistBlueprint => "Heist Blueprint",
    HeistContract => "Heist Contract",
    HeistTool => "Heist Tool",
    HeistBrooch => "Heist Brooch",
    HeistGear => "Heist Gear",
    HeistCloak => "Heist Cloak",
    Trinket => "Trinket",
    Invitation => "Invitation",
    ExpeditionLogbook => "Expedition Logbook",
    Gem => "Gem",
    Currency => "Currency",
    DivinationCard => "Divination Card",
    Voidstone => "Voidstone",
    Sentinel => "Sentinel",
    MemoryLine => "Memory Line",
    SanctumRelic => "Sanctum Relic",
    Tincture => "Tincture",
    Charm => "Charm",
    Idol => "Idol",
    Graft => "Graft",
    Chart => "Chart",
    MercenaryWarrant => "Mercenary Warrant",
    Viscera => "Viscera",
}

/// A category as the data files and the parser use it.
///
/// The data also holds categories that `ItemCategory` doesn't list (like "Unique Fragment"); they are kept as
/// plain strings that never match any comparison.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Category {
    Known(ItemCategory),
    Other(String),
}

impl Category {
    #[must_use]
    pub const fn known(&self) -> Option<ItemCategory> {
        match self {
            Self::Known(c) => Some(*c),
            Self::Other(_) => None,
        }
    }
}

impl From<ItemCategory> for Category {
    fn from(c: ItemCategory) -> Self {
        Self::Known(c)
    }
}

impl Default for Category {
    fn default() -> Self {
        Self::Other(String::new())
    }
}

impl Serialize for Category {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Known(c) => s.serialize_str(c.as_str()),
            Self::Other(o) => s.serialize_str(o),
        }
    }
}

impl<'de> Deserialize<'de> for Category {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let name = String::deserialize(d)?;
        Ok(ItemCategory::from_name(&name)
            .map(Category::Known)
            .unwrap_or(Self::Other(name)))
    }
}

/// `item.category === X` where `item.category` may be unset.
#[must_use]
pub fn is(category: &Option<Category>, wanted: ItemCategory) -> bool {
    matches!(category, Some(Category::Known(c)) if *c == wanted)
}

/// The known category, if the item has one.
pub fn known(category: &Option<Category>) -> Option<ItemCategory> {
    category.as_ref().and_then(Category::known)
}

use ItemCategory::{
    Amulet, Belt, BodyArmour, Boots, Bow, Claw, Dagger, FishingRod, Gloves, HeistBrooch, HeistCloak, HeistGear,
    HeistTool, Helmet, OneHandedAxe, OneHandedMace, OneHandedSword, Ring, RuneDagger, Sceptre, Shield, Staff,
    TwoHandedAxe, TwoHandedMace, TwoHandedSword, Wand, Warstaff,
};

#[must_use]
pub const fn is_weapon(c: ItemCategory) -> bool {
    matches!(
        c,
        FishingRod
            | Bow
            | Wand
            | OneHandedAxe
            | OneHandedMace
            | OneHandedSword
            | Claw
            | Dagger
            | RuneDagger
            | Sceptre
            | TwoHandedAxe
            | TwoHandedMace
            | TwoHandedSword
            | Staff
            | Warstaff
    )
}

#[must_use]
pub const fn is_armour(c: ItemCategory) -> bool {
    matches!(c, BodyArmour | Boots | Gloves | Helmet | Shield)
}

#[must_use]
pub const fn is_jewellery(c: ItemCategory) -> bool {
    matches!(c, Amulet | Belt | Ring)
}

#[must_use]
pub const fn is_heist_equipment(c: ItemCategory) -> bool {
    matches!(c, HeistTool | HeistBrooch | HeistGear | HeistCloak)
}
