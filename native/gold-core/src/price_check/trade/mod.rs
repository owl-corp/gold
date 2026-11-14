//! The official trade site: building searches, rate limiting, search and fetch
//!.
pub mod bulk;
pub mod client;
pub mod limits;
pub mod request;

use crate::parser::meta::ItemCategory;
use serde::Deserialize;

/// The seller, as the trade API reports them.
#[derive(Deserialize)]
pub(super) struct Account {
    pub name: String,
    #[serde(rename = "lastCharacterName", default)]
    pub last_character_name: String,
    #[serde(default)]
    online: Option<Online>,
}

#[derive(Deserialize)]
struct Online {
    #[serde(default)]
    status: Option<String>,
}

impl Account {
    /// "online", "afk" or "offline"
    pub fn status(&self) -> &'static str {
        match &self.online {
            Some(o) if o.status.as_deref() == Some("afk") => "afk",
            Some(_) => "online",
            None => "offline",
        }
    }
}

/// `CATEGORY_TO_TRADE_ID`: the trade site's category option for an item category.
pub fn category_to_trade_id(c: ItemCategory) -> Option<&'static str> {
    use ItemCategory::*;
    Some(match c {
        Map => "map",
        AbyssJewel => "jewel.abyss",
        Amulet => "accessory.amulet",
        Belt => "accessory.belt",
        BodyArmour => "armour.chest",
        Boots => "armour.boots",
        Bow => "weapon.bow",
        Claw => "weapon.claw",
        Dagger => "weapon.dagger",
        FishingRod => "weapon.rod",
        Flask => "flask",
        Gloves => "armour.gloves",
        Helmet => "armour.helmet",
        Jewel => "jewel",
        OneHandedAxe => "weapon.oneaxe",
        OneHandedMace => "weapon.onemace",
        OneHandedSword => "weapon.onesword",
        Quiver => "armour.quiver",
        Ring => "accessory.ring",
        RuneDagger => "weapon.runedagger",
        Sceptre => "weapon.sceptre",
        Shield => "armour.shield",
        Staff => "weapon.staff",
        TwoHandedAxe => "weapon.twoaxe",
        TwoHandedMace => "weapon.twomace",
        TwoHandedSword => "weapon.twosword",
        Wand => "weapon.wand",
        Warstaff => "weapon.warstaff",
        ClusterJewel => "jewel.cluster",
        HeistBlueprint => "heistmission.blueprint",
        HeistContract => "heistmission.contract",
        HeistTool => "heistequipment.heisttool",
        HeistBrooch => "heistequipment.heistreward",
        HeistGear => "heistequipment.heistweapon",
        HeistCloak => "heistequipment.heistutility",
        Trinket => "accessory.trinket",
        SanctumRelic => "sanctum.relic",
        Tincture => "tincture",
        Charm => "azmeri.charm",
        Idol => "idol",
        Graft => "graft",
        Chart => "chart",
        _ => return None,
    })
}
