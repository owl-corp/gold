//! Reads `data/<lang>/client_strings.js` (the game's UI strings and regexes)
//! directly, so no JavaScript runtime is needed at build or run time.
use anyhow::{anyhow, bail, Context, Result};
use regex::{Regex, RegexBuilder};
use std::collections::HashMap;

enum Entry {
    Str(String),
    Re(Regex),
    List(Vec<String>),
}

macro_rules! client_strings {
    (str: [$($s:ident),* $(,)?], re: [$($r:ident),* $(,)?], list: [$($l:ident),* $(,)?] $(,)?) => {
        #[allow(non_snake_case)]
        pub struct ClientStrings {
            $(pub $s: String,)*
            $(pub $r: Regex,)*
            $(pub $l: Vec<String>,)*
        }

        impl ClientStrings {
            fn from_entries(mut map: HashMap<String, Entry>) -> Result<Self> {
                Ok(ClientStrings {
                    $($s: match map.remove(stringify!($s)) {
                        Some(Entry::Str(v)) => v,
                        _ => bail!("client_strings: {} missing or not a string", stringify!($s)),
                    },)*
                    $($r: match map.remove(stringify!($r)) {
                        Some(Entry::Re(v)) => v,
                        _ => bail!("client_strings: {} missing or not a regex", stringify!($r)),
                    },)*
                    $($l: match map.remove(stringify!($l)) {
                        Some(Entry::List(v)) => v,
                        _ => bail!("client_strings: {} missing or not a list", stringify!($l)),
                    },)*
                })
            }
        }
    };
}

client_strings! {
    str: [
        RARITY_NORMAL, RARITY_MAGIC, RARITY_RARE, RARITY_UNIQUE, RARITY_GEM, RARITY_CURRENCY,
        RARITY_DIVCARD, RARITY_QUEST, MAP_ITEM_QUANTITY, MAP_ITEM_RARITY, MAP_MONSTER_PACK_SIZE,
        MAP_MORE_MAPS, MAP_MORE_SCARABS, MAP_MORE_CURRENCY, MAP_MORE_DIVINATION_CARDS, RARITY,
        ITEM_CLASS, ITEM_LEVEL, CORPSE_LEVEL, TALISMAN_TIER, GEM_LEVEL, STACK_SIZE, SOCKETS,
        QUALITY, MEMORY_STRANDS, PHYSICAL_DAMAGE, ELEMENTAL_DAMAGE, CRIT_CHANCE, ATTACK_SPEED,
        ARMOUR, EVASION, ENERGY_SHIELD, TAG_WARD, BLOCK_CHANCE, CORRUPTED, UNIDENTIFIED,
        INFLUENCE_SHAPER, INFLUENCE_ELDER, INFLUENCE_CRUSADER, INFLUENCE_HUNTER,
        INFLUENCE_REDEEMER, INFLUENCE_WARLORD, SECTION_SYNTHESISED, VEILED_PREFIX, VEILED_SUFFIX,
        METAMORPH_HELP, BEAST_HELP, VOIDSTONE_HELP, CANNOT_USE_ITEM, AREA_LEVEL,
        HEIST_WINGS_REVEALED, HEIST_BLUEPRINT_TARGET, HEIST_BLUEPRINT_ENCHANTS,
        HEIST_BLUEPRINT_TRINKETS, HEIST_BLUEPRINT_GEMS, HEIST_BLUEPRINT_REPLICAS,
        HEIST_JOB_LOCKPICKING, HEIST_JOB_BRUTEFORCE, HEIST_JOB_PERCEPTION, HEIST_JOB_DEMOLITION,
        HEIST_JOB_COUNTERTHAUMATURGY, HEIST_JOB_TRAPDISARMAMENT, HEIST_JOB_AGILITY,
        HEIST_JOB_DECEPTION, HEIST_JOB_ENGINEERING, HEIST_TARGET_PRICELESS, MIRRORED, SPLIT,
        PREFIX_MODIFIER, SUFFIX_MODIFIER, CRAFTED_PREFIX, CRAFTED_SUFFIX, IMPLICIT_MODIFIER,
        FRACTURED_PREFIX, FRACTURED_SUFFIX, UNSCALABLE_VALUE, CORRUPTED_IMPLICIT, INCURSION_OPEN,
        INCURSION_OBSTRUCTED, ITEM_EATER, ITEM_EXARCH, ELDRITCH_MOD_R1, ELDRITCH_MOD_R2,
        ELDRITCH_MOD_R3, ELDRITCH_MOD_R4, ELDRITCH_MOD_R5, ELDRITCH_MOD_R6, SENTINEL_CHARGE,
        FOIL_UNIQUE, UNMODIFIABLE, FOULBORN_MODIFIER, VESTIGIAL_IMPLICIT, MAP_AREA, CHART_SULPHUR,
        MERCENARY_LEVEL, MERCENARY_BUILD,
    ],
    re: [
        MAP_TIER, MAP_COMPLETION_REWARD, ITEM_SUPERIOR, MAP_BLIGHTED, MAP_BLIGHT_RAVAGED,
        ITEM_SYNTHESISED, FLASK_CHARGES, METAMORPH_BRAIN, METAMORPH_EYE, METAMORPH_LUNG,
        METAMORPH_HEART, METAMORPH_LIVER, QUALITY_ANOMALOUS, QUALITY_DIVERGENT,
        QUALITY_PHANTASMAL, HEIST_CONTRACT_JOB, HEIST_CONTRACT_TARGET, MODIFIER_LINE,
        MODIFIER_INCREASED, EATER_IMPLICIT, EXARCH_IMPLICIT, FOULBORN_NAME, VESTIGIAL_NAME,
        CHAT_SYSTEM, CHAT_TRADE, CHAT_GLOBAL, CHAT_PARTY, CHAT_GUILD, CHAT_WHISPER_TO,
        CHAT_WHISPER_FROM, CHAT_WEBTRADE_GEM,
    ],
    list: [
        SHAPER_MODS, ELDER_MODS, CRUSADER_MODS, HUNTER_MODS, REDEEMER_MODS, WARLORD_MODS,
        DELVE_MODS, VEILED_MODS, INCURSION_MODS, ESSENCE_MODS, INFAMOUS_MODS,
    ],
}

impl ClientStrings {
    pub fn parse(source: &str) -> Result<Self> {
        let mut p = Scanner {
            s: source.as_bytes(),
            text: source,
            i: 0,
        };
        let start = source.find("export default").context("no `export default`")?;
        p.i = start;
        p.skip_to(b'{')?;
        p.i += 1;

        let mut map = HashMap::new();
        loop {
            p.skip_ws_and_comments();
            if p.peek() == Some(b'}') {
                break;
            }
            let key = p.ident()?;
            p.skip_ws_and_comments();
            p.expect(b':')?;
            p.skip_ws_and_comments();
            let value = p.value().with_context(|| format!("value of {key}"))?;
            map.insert(key, value);
            p.skip_ws_and_comments();
            if p.peek() == Some(b',') {
                p.i += 1;
            }
        }
        Self::from_entries(map)
    }
}

struct Scanner<'a> {
    s: &'a [u8],
    text: &'a str,
    i: usize,
}

impl Scanner<'_> {
    fn peek(&self) -> Option<u8> {
        self.s.get(self.i).copied()
    }

    fn expect(&mut self, c: u8) -> Result<()> {
        if self.peek() == Some(c) {
            self.i += 1;
            Ok(())
        } else {
            Err(anyhow!("expected `{}` at byte {}", c as char, self.i))
        }
    }

    fn skip_to(&mut self, c: u8) -> Result<()> {
        while self.peek().ok_or_else(|| anyhow!("unexpected end"))? != c {
            self.i += 1;
        }
        Ok(())
    }

    fn skip_ws_and_comments(&mut self) {
        loop {
            match self.peek() {
                Some(c) if c.is_ascii_whitespace() => self.i += 1,
                Some(b'/') if self.s.get(self.i + 1) == Some(&b'/') => {
                    while self.peek().is_some_and(|c| c != b'\n') {
                        self.i += 1;
                    }
                }
                _ => break,
            }
        }
    }

    fn ident(&mut self) -> Result<String> {
        let start = self.i;
        while self.peek().is_some_and(|c| c.is_ascii_alphanumeric() || c == b'_') {
            self.i += 1;
        }
        if start == self.i {
            bail!("expected identifier at byte {start}");
        }
        Ok(self.text[start..self.i].to_string())
    }

    fn value(&mut self) -> Result<Entry> {
        match self.peek() {
            Some(b'\'' | b'"') => Ok(Entry::Str(self.string()?)),
            Some(b'/') => Ok(Entry::Re(self.regex()?)),
            Some(b'[') => {
                self.i += 1;
                let mut out = Vec::new();
                loop {
                    self.skip_ws_and_comments();
                    match self.peek() {
                        Some(b']') => {
                            self.i += 1;
                            break;
                        }
                        Some(b',') => self.i += 1,
                        Some(b'\'' | b'"') => out.push(self.string()?),
                        _ => bail!("unexpected token in array at byte {}", self.i),
                    }
                }
                Ok(Entry::List(out))
            }
            _ => bail!("unsupported value at byte {}", self.i),
        }
    }

    fn string(&mut self) -> Result<String> {
        let quote = self.peek().unwrap();
        self.i += 1;
        let mut out = String::new();
        loop {
            let rest = &self.text[self.i..];
            let c = rest.chars().next().ok_or_else(|| anyhow!("unterminated string"))?;
            self.i += c.len_utf8();
            if c as u32 == quote as u32 {
                return Ok(out);
            }
            if c == '\\' {
                let e = self.text[self.i..]
                    .chars()
                    .next()
                    .ok_or_else(|| anyhow!("bad escape"))?;
                self.i += e.len_utf8();
                out.push(match e {
                    'n' => '\n',
                    't' => '\t',
                    other => other,
                });
            } else {
                out.push(c);
            }
        }
    }

    fn regex(&mut self) -> Result<Regex> {
        self.i += 1;
        let start = self.i;
        let mut in_class = false;
        loop {
            match self.peek().ok_or_else(|| anyhow!("unterminated regex"))? {
                b'\\' => self.i += 2,
                b'[' => {
                    in_class = true;
                    self.i += 1;
                }
                b']' => {
                    in_class = false;
                    self.i += 1;
                }
                b'/' if !in_class => break,
                _ => self.i += 1,
            }
        }
        let source = &self.text[start..self.i];
        self.i += 1;
        let flags_start = self.i;
        while self.peek().is_some_and(|c| c.is_ascii_alphabetic()) {
            self.i += 1;
        }
        let flags = &self.text[flags_start..self.i];
        RegexBuilder::new(source)
            .case_insensitive(flags.contains('i'))
            .multi_line(flags.contains('m'))
            .dot_matches_new_line(flags.contains('s'))
            .build()
            .with_context(|| format!("regex /{source}/"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_english_client_strings() {
        let src = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../data/en/client_strings.js"
        ))
        .unwrap();
        let cs = ClientStrings::parse(&src).unwrap();
        assert_eq!(cs.ITEM_CLASS, "Item Class: ");
        assert_eq!(cs.HEIST_BLUEPRINT_TRINKETS, "Thieves' Trinkets or Currency");
        assert_eq!(cs.UNSCALABLE_VALUE, " \u{2014} Unscalable Value");
        assert_eq!(&cs.MAP_TIER.captures(" (Tier 14)").unwrap()[1], "14");
        let m = cs
            .HEIST_CONTRACT_JOB
            .captures("Requires Lockpicking (Level 3)")
            .unwrap();
        assert_eq!(&m["job"], "Lockpicking");
        assert_eq!(cs.SHAPER_MODS.len(), 4);
    }
}
