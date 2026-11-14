//! Item categories and category groups.
use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ItemCategory {
    Map,
    CapturedBeast,
    MetamorphSample,
    Helmet,
    BodyArmour,
    Gloves,
    Boots,
    Shield,
    Amulet,
    Belt,
    Ring,
    Flask,
    AbyssJewel,
    Jewel,
    Quiver,
    Claw,
    Bow,
    Sceptre,
    Wand,
    FishingRod,
    Staff,
    Warstaff,
    Dagger,
    RuneDagger,
    OneHandedAxe,
    TwoHandedAxe,
    OneHandedMace,
    TwoHandedMace,
    OneHandedSword,
    TwoHandedSword,
    ClusterJewel,
    HeistBlueprint,
    HeistContract,
    HeistTool,
    HeistBrooch,
    HeistGear,
    HeistCloak,
    Trinket,
    Invitation,
    ExpeditionLogbook,
    Gem,
    Currency,
    DivinationCard,
    Voidstone,
    Sentinel,
    MemoryLine,
    SanctumRelic,
    Tincture,
    Charm,
    Idol,
    Graft,
    Chart,
    MercenaryWarrant,
}

impl ItemCategory {
    pub fn as_str(self) -> &'static str {
        match self {
            ItemCategory::Map => "Map",
            ItemCategory::CapturedBeast => "Captured Beast",
            ItemCategory::MetamorphSample => "Metamorph Sample",
            ItemCategory::Helmet => "Helmet",
            ItemCategory::BodyArmour => "Body Armour",
            ItemCategory::Gloves => "Gloves",
            ItemCategory::Boots => "Boots",
            ItemCategory::Shield => "Shield",
            ItemCategory::Amulet => "Amulet",
            ItemCategory::Belt => "Belt",
            ItemCategory::Ring => "Ring",
            ItemCategory::Flask => "Flask",
            ItemCategory::AbyssJewel => "Abyss Jewel",
            ItemCategory::Jewel => "Jewel",
            ItemCategory::Quiver => "Quiver",
            ItemCategory::Claw => "Claw",
            ItemCategory::Bow => "Bow",
            ItemCategory::Sceptre => "Sceptre",
            ItemCategory::Wand => "Wand",
            ItemCategory::FishingRod => "Fishing Rod",
            ItemCategory::Staff => "Staff",
            ItemCategory::Warstaff => "Warstaff",
            ItemCategory::Dagger => "Dagger",
            ItemCategory::RuneDagger => "Rune Dagger",
            ItemCategory::OneHandedAxe => "One-Handed Axe",
            ItemCategory::TwoHandedAxe => "Two-Handed Axe",
            ItemCategory::OneHandedMace => "One-Handed Mace",
            ItemCategory::TwoHandedMace => "Two-Handed Mace",
            ItemCategory::OneHandedSword => "One-Handed Sword",
            ItemCategory::TwoHandedSword => "Two-Handed Sword",
            ItemCategory::ClusterJewel => "Cluster Jewel",
            ItemCategory::HeistBlueprint => "Heist Blueprint",
            ItemCategory::HeistContract => "Heist Contract",
            ItemCategory::HeistTool => "Heist Tool",
            ItemCategory::HeistBrooch => "Heist Brooch",
            ItemCategory::HeistGear => "Heist Gear",
            ItemCategory::HeistCloak => "Heist Cloak",
            ItemCategory::Trinket => "Trinket",
            ItemCategory::Invitation => "Invitation",
            ItemCategory::ExpeditionLogbook => "Expedition Logbook",
            ItemCategory::Gem => "Gem",
            ItemCategory::Currency => "Currency",
            ItemCategory::DivinationCard => "Divination Card",
            ItemCategory::Voidstone => "Voidstone",
            ItemCategory::Sentinel => "Sentinel",
            ItemCategory::MemoryLine => "Memory Line",
            ItemCategory::SanctumRelic => "Sanctum Relic",
            ItemCategory::Tincture => "Tincture",
            ItemCategory::Charm => "Charm",
            ItemCategory::Idol => "Idol",
            ItemCategory::Graft => "Graft",
            ItemCategory::Chart => "Chart",
            ItemCategory::MercenaryWarrant => "Mercenary Warrant",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "Map" => ItemCategory::Map,
            "Captured Beast" => ItemCategory::CapturedBeast,
            "Metamorph Sample" => ItemCategory::MetamorphSample,
            "Helmet" => ItemCategory::Helmet,
            "Body Armour" => ItemCategory::BodyArmour,
            "Gloves" => ItemCategory::Gloves,
            "Boots" => ItemCategory::Boots,
            "Shield" => ItemCategory::Shield,
            "Amulet" => ItemCategory::Amulet,
            "Belt" => ItemCategory::Belt,
            "Ring" => ItemCategory::Ring,
            "Flask" => ItemCategory::Flask,
            "Abyss Jewel" => ItemCategory::AbyssJewel,
            "Jewel" => ItemCategory::Jewel,
            "Quiver" => ItemCategory::Quiver,
            "Claw" => ItemCategory::Claw,
            "Bow" => ItemCategory::Bow,
            "Sceptre" => ItemCategory::Sceptre,
            "Wand" => ItemCategory::Wand,
            "Fishing Rod" => ItemCategory::FishingRod,
            "Staff" => ItemCategory::Staff,
            "Warstaff" => ItemCategory::Warstaff,
            "Dagger" => ItemCategory::Dagger,
            "Rune Dagger" => ItemCategory::RuneDagger,
            "One-Handed Axe" => ItemCategory::OneHandedAxe,
            "Two-Handed Axe" => ItemCategory::TwoHandedAxe,
            "One-Handed Mace" => ItemCategory::OneHandedMace,
            "Two-Handed Mace" => ItemCategory::TwoHandedMace,
            "One-Handed Sword" => ItemCategory::OneHandedSword,
            "Two-Handed Sword" => ItemCategory::TwoHandedSword,
            "Cluster Jewel" => ItemCategory::ClusterJewel,
            "Heist Blueprint" => ItemCategory::HeistBlueprint,
            "Heist Contract" => ItemCategory::HeistContract,
            "Heist Tool" => ItemCategory::HeistTool,
            "Heist Brooch" => ItemCategory::HeistBrooch,
            "Heist Gear" => ItemCategory::HeistGear,
            "Heist Cloak" => ItemCategory::HeistCloak,
            "Trinket" => ItemCategory::Trinket,
            "Invitation" => ItemCategory::Invitation,
            "Expedition Logbook" => ItemCategory::ExpeditionLogbook,
            "Gem" => ItemCategory::Gem,
            "Currency" => ItemCategory::Currency,
            "Divination Card" => ItemCategory::DivinationCard,
            "Voidstone" => ItemCategory::Voidstone,
            "Sentinel" => ItemCategory::Sentinel,
            "Memory Line" => ItemCategory::MemoryLine,
            "Sanctum Relic" => ItemCategory::SanctumRelic,
            "Tincture" => ItemCategory::Tincture,
            "Charm" => ItemCategory::Charm,
            "Idol" => ItemCategory::Idol,
            "Graft" => ItemCategory::Graft,
            "Chart" => ItemCategory::Chart,
            "Mercenary Warrant" => ItemCategory::MercenaryWarrant,
            _ => return None,
        })
    }
}

/// A category as the data files and the parser use it. The data also holds categories
/// that `ItemCategory` doesn't list (like "Unique Fragment"); they are kept as
/// plain strings that never match any comparison.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Category {
    Known(ItemCategory),
    Other(String),
}

impl Category {
    pub fn known(&self) -> Option<ItemCategory> {
        match self {
            Category::Known(c) => Some(*c),
            Category::Other(_) => None,
        }
    }
}

impl From<ItemCategory> for Category {
    fn from(c: ItemCategory) -> Self {
        Category::Known(c)
    }
}

impl Default for Category {
    fn default() -> Self {
        Category::Other(String::new())
    }
}

impl Serialize for Category {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            Category::Known(c) => s.serialize_str(c.as_str()),
            Category::Other(o) => s.serialize_str(o),
        }
    }
}

impl<'de> Deserialize<'de> for Category {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let name = String::deserialize(d)?;
        Ok(ItemCategory::from_name(&name)
            .map(Category::Known)
            .unwrap_or(Category::Other(name)))
    }
}

/// `item.category === X` where `item.category` may be unset.
pub fn is(category: &Option<Category>, wanted: ItemCategory) -> bool {
    matches!(category, Some(Category::Known(c)) if *c == wanted)
}

/// The known category, if the item has one.
pub fn known(category: &Option<Category>) -> Option<ItemCategory> {
    category.as_ref().and_then(Category::known)
}

use ItemCategory::*;

pub fn is_weapon(c: ItemCategory) -> bool {
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

pub fn is_armour(c: ItemCategory) -> bool {
    matches!(c, BodyArmour | Boots | Gloves | Helmet | Shield)
}

pub fn is_jewellery(c: ItemCategory) -> bool {
    matches!(c, Amulet | Belt | Ring)
}

pub fn is_heist_equipment(c: ItemCategory) -> bool {
    matches!(c, HeistTool | HeistBrooch | HeistGear | HeistCloak)
}
