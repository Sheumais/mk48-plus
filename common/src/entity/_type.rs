use crate::altitude::Altitude;
use crate::entity::{
    Armament, EntityData, EntityKind, EntitySubKind, Exhaust, Sensor, Sensors, Turret,
};
use crate::ticks::Ticks;
use crate::util::{level_to_score, natural_death_coins};
use crate::velocity::Velocity;
use arrayvec::ArrayVec;
use common_util::angle::Angle;
use core_protocol::serde_util::{StrVisitor, U8Visitor};
use macros::EntityTypeData;
use rand::prelude::IteratorRandom;
use rand::{thread_rng, Rng};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

impl EntityType {
    /// Data returns the data associated with the entity type.
    #[inline]
    pub fn data(self) -> &'static EntityData {
        unsafe { Self::DATA.get_unchecked(self as usize) }
    }

    /// reduced lifespan returns a lifespan to start an entity's life at, so as to make it expire
    /// in desired_lifespan ticks
    pub fn reduced_lifespan(self, desired_lifespan: Ticks) -> Ticks {
        self.data().lifespan.saturating_sub(desired_lifespan)
    }

    /// can_spawn_as returns whether it is possible to spawn as the entity type, which may depend
    /// on whether you are a bot.
    pub fn can_spawn_as(self, score: u32, bot: bool, moderator: bool) -> bool {
        let data = self.data();
        if (bot || !moderator) && data.sub_kind == EntitySubKind::Drone {return false};
        data.kind == EntityKind::Boat && level_to_score(data.level) <= score && (bot || !data.npc)
    }

    /// can_upgrade_to returns whether it is possible to upgrade to the entity type, which may depend
    /// on your score and whether you are a bot.
    pub fn can_upgrade_to(self, upgrade: Self, score: u32, bot: bool, moderator: bool) -> bool {
        let data = self.data();
        let upgrade_data = upgrade.data();
        if moderator && upgrade_data.kind == data.kind {return true};
        if upgrade_data.sub_kind == EntitySubKind::Drone && !moderator {return false};
        if bot && upgrade == EntityType::Chinook {return false};
        if bot && upgrade == EntityType::Lst {return false};
        if self == EntityType::Lst && upgrade == EntityType::Sherman {return score < level_to_score(6) && score >= level_to_score(4)};
        if data.sub_kind == EntitySubKind::Tank && upgrade_data.sub_kind == EntitySubKind::LandingShip {return true};
        if data.sub_kind == EntitySubKind::LandingShip && upgrade_data.sub_kind == EntitySubKind::Tank {return true};
        upgrade_data.level > data.level 
            && upgrade_data.kind == data.kind
            && score >= level_to_score(upgrade_data.level)
            && (bot || !upgrade_data.npc)
    }

    /// iter returns an iterator that visits all possible entity types and allows a random choice to
    /// be made.
    pub fn iter() -> impl Iterator<Item = Self> + IteratorRandom {
        use enum_iterator::IntoEnumIterator;
        Self::into_enum_iter()
    }

    /// spawn_options returns an iterator that visits all spawnable entity types and allows a random
    /// choice to be made.
    pub fn spawn_options(score: u32, bot: bool, moderator: bool) -> impl Iterator<Item = Self> + IteratorRandom {
        Self::iter().filter(move |t| t.can_spawn_as(score, bot, moderator))
    }

    /// upgrade_options returns an iterator that visits all entity types that may be upgraded to
    /// and allows a random choice to be made.
    #[inline]
    pub fn upgrade_options(
        self,
        score: u32,
        bot: bool,
        moderator: bool,
    ) -> impl Iterator<Item = Self> + IteratorRandom {
        // Don't iterate if not enough score for next level.
         
        if score >= level_to_score(self.data().level) || (self.data().sub_kind == EntitySubKind::Tank || self.data().sub_kind == EntitySubKind::LandingShip) || moderator {
            Some(Self::iter().filter(move |t| self.can_upgrade_to(*t, score, bot, moderator)))
        } else {
            None
        }
        .into_iter()
        .flatten()
    }

    /// iterates all loot types entity should drop. Takes score before death.
    pub fn loot(self, score: u32, score_to_coins: bool) -> impl Iterator<Item = Self> + 'static {
        let data: &EntityData = self.data();

        debug_assert_eq!(data.kind, EntityKind::Boat);

        let coin_amount = if score_to_coins {
            natural_death_coins(score)
        } else {
            0
        };

        let mut rng = thread_rng();

        // Loot is based on the length of the boat.
        let loot_amount = (data.length * 0.25 * (rng.gen::<f32>() * 0.1 + 0.9)) as u32;

        let mut loot_table = ArrayVec::<Self, 4>::new();

        match data.sub_kind {
            EntitySubKind::Pirate => {
                loot_table.push(Self::Crate);
                loot_table.push(Self::Coin);
            }
            EntitySubKind::Tanker => {
                loot_table.push(Self::Scrap);
                loot_table.push(Self::Barrel);
            }
            _ => match self {
                Self::Olympias => loot_table.push(Self::Crate),
                _ => loot_table.push(Self::Scrap),
            },
        };

        (0..loot_amount)
            .map(move |_| {
                *loot_table
                    .iter()
                    .choose(&mut rng)
                    .expect("at least once loot table option")
            })
            .chain((0..coin_amount).map(|_| Self::Coin))
    }
}

impl Serialize for EntityType {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        if serializer.is_human_readable() {
            serializer.serialize_str(self.as_str())
        } else {
            debug_assert_eq!(Self::from_u8(*self as u8).unwrap(), *self);
            serializer.serialize_u8(*self as u8)
        }
    }
}

impl<'de> Deserialize<'de> for EntityType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        if deserializer.is_human_readable() {
            deserializer.deserialize_str(StrVisitor).and_then(|s| {
                Self::from_str(s.as_str()).ok_or_else(|| {
                    serde::de::Error::custom(format!("invalid entity type {}", s.as_str()))
                })
            })
        } else {
            deserializer.deserialize_u8(U8Visitor).and_then(|i| {
                Self::from_u8(i).ok_or_else(|| {
                    serde::de::Error::custom(format!("invalid entity type integer {}", i))
                })
            })
        }
    }
}

#[repr(u8)]
#[derive(
    Copy,
    Clone,
    Eq,
    PartialEq,
    Ord,
    PartialOrd,
    Hash,
    enum_iterator::IntoEnumIterator,
    EntityTypeData,
)]
pub enum EntityType {
    #[info(
        label = "M1 Abrams",
        link = "https://en.wikipedia.org/wiki/M1_Abrams"
    )]
    #[entity(Boat, Tank, level = 6)]
    #[size(length = 7.93, width = 3.66, draft = 1.0)]
    #[props(speed = 13.333, ram_damage = 3)]
    #[sensors(visual = 700, radar = 700)]
    #[turret(AbramsTurret, fast)]
    Abrams,
    #[info(
        label = "TBF Avenger",
        link = "https://en.wikipedia.org/wiki/Grumman_TBF_Avenger"
    )]
    #[entity(Aircraft, Plane, level = 5)]
    #[size(length = 12.45, width = 16.5143)]
    #[props(speed = 96.1136, range = 1456000)]
    #[sensors(visual)]
    #[armament(Mark18)]
    Avenger,
    #[info(
        label = "Shenyang J-15",
        link = "https://en.wikipedia.org/wiki/Shenyang_J-15"
    )]
    #[entity(Aircraft, Plane, level = 9)]
    #[size(length = 22.28, width = 15.0)]
    #[props(speed = 343, range = 350000)]
    #[sensors(visual)]
    #[armament(Yj18)]
    J15,
    #[info(
        label = "Nakajima E4N",
        link = "https://en.wikipedia.org/wiki/Nakajima_E4N"
    )]
    #[entity(Aircraft, Plane, level = 5)]
    #[size(length = 8.87, width = 10.25)]
    #[props(speed = 41.15, range = 1019000)]
    #[sensors(visual)]
    #[armament(Mark18)]
    E4N,
    #[info(
        label = "TIE Starfighter",
        link = "https://starwars.fandom.com/wiki/TIE/ln_space_superiority_starfighter"
    )]
    #[entity(Aircraft, Plane, level = 12)]
    #[size(length = 7.24, width = 6.7)]
    #[props(speed = 333.333, range = 1000000)]
    #[sensors(visual)]
    #[armament(GreenBlaster)]
    TieFighter, //"3D T.I.E Fighter - Star Wars model" (https://skfb.ly/Q98Y) by Mickael Boitte is licensed under Creative Commons Attribution (http://creativecommons.org/licenses/by/4.0/).
    #[info(
        label = "Harbin Z-9",
        link = "https://en.wikipedia.org/wiki/Harbin_Z-9"
    )]
    #[entity(Aircraft, Heli, level = 5)]
    #[size(length = 14.2143, width = 13.1038)]
    #[props(speed = 72.02226, range = 1000000)]
    #[sensors(visual)]
    #[armament(_82R, forward = 1, side = 1, symmetrical)]
    Harbin,
    #[info(label = "Ka-25", link = "https://en.wikipedia.org/wiki/Kamov_Ka-25")]
    #[entity(Aircraft, Heli, level = 5)]
    #[size(length = 15.8, width = 15.8)]
    #[props(speed = 53.50225, range = 400000)]
    #[sensors(visual)]
    #[armament(_82R, side = 1, symmetrical)]
    Ka25,
    #[info(
        label = "Kingfisher",
        link = "https://en.wikipedia.org/wiki/Vought_OS2U_Kingfisher"
    )]
    #[entity(Aircraft, Plane, level = 5)]
    #[size(length = 10.0853, width = 10.94)]
    #[props(speed = 67.9067, range = 1461000)]
    #[sensors(visual)]
    #[armament(Mark18)]
    Kingfisher,
    #[info(
        label = "Seahawk",
        link = "https://en.wikipedia.org/wiki/Sikorsky_SH-60_Seahawk"
    )]
    #[entity(Aircraft, Heli, level = 5)]
    #[size(length = 19.6, width = 16.078)]
    #[props(speed = 75.1089, range = 5000)]
    #[sensors(visual)]
    #[armament(Mark54, side = 1, symmetrical)]
    Seahawk,
    #[info(
        label = "Mitsubishi A5M",
        link = "https://en.wikipedia.org/wiki/Mitsubishi_A5M"
    )]
    #[entity(Aircraft, Plane, level = 7)]
    #[size(length = 7.565, width = 11)]
    #[props(speed = 334.7222, range = 1201000)]
    #[sensors(visual)]
    #[armament(Type96Bomb)]
    Type96,
    #[info(
        label = "Super Étendard",
        link = "https://en.wikipedia.org/wiki/Dassault-Breguet_Super_%C3%89tendard"
    )]
    #[entity(Aircraft, Plane, level = 7)]
    #[size(length = 14.31, width = 9.4468)]
    #[props(speed = 334.7222, range = 1820000)]
    #[sensors(visual)]
    #[armament(Exocet)]
    #[armament(Magic, forward = -1.75, side = 2.2)]
    SuperEtendard,
    #[info(
        label = "Super Frelon",
        link = "https://en.wikipedia.org/wiki/A%C3%A9rospatiale_SA_321_Super_Frelon"
    )]
    #[entity(Aircraft, Heli, level = 5)]
    #[size(length = 23.1, width = 18.949)]
    #[props(speed = 69, range = 1020000)]
    #[sensors(visual)]
    #[armament(Mark54, side = 0.75, symmetrical)]
    SuperFrelon,
    #[info(
        label = "Changhe Z-18",
        link = "https://en.wikipedia.org/wiki/Changhe_Z-18"
    )]
    #[entity(Aircraft, Heli, level = 9)]
    #[size(length = 23.1, width = 18.949)]
    #[props(speed = 69, range = 1020000)]
    #[sensors(visual)]
    #[armament(Yu7, side = 0.0)]
    Z18,
    #[info(
        label = "Akula",
        link = "https://en.wikipedia.org/wiki/Akula-class_submarine"
    )]
    #[entity(Boat, Submarine, level = 6)]
    #[size(length = 113.3, width = 20.137, draft = 8.14, mast = 8.81)]
    #[props(speed = 18.00556, depth = 480)]
    #[sensors(sonar, visual)]
    #[armament(Set65, forward = 50.5, side = 1.5, angle = 0, count = 2, symmetrical)]
    #[armament(Set65, forward = 51, side = 0.6, angle = 0, count = 2, symmetrical)]
    #[armament(Igla, forward = 4.86495, count = 2, vertical)]
    #[armament(Brosok, forward = 52, side = 0.5, angle = 0, symmetrical)]
    Akula,
    #[info(
        label = "AH-64 Apache",
        link = "https://en.wikipedia.org/wiki/Boeing_AH-64_Apache"
    )]
    #[entity(Boat, Helicopter, level = 7)]
    #[size(length = 17.73, width = 14.63, draft = 0.0)]
    #[props(speed = 81.282)]
    #[sensors(visual = 700, radar = 700)]
    #[armament(Hellfire, forward = 5.0, side = 3.0, symmetrical, hidden)]
    #[armament(Hellfire, forward = 5.0, side = 5.0, symmetrical, hidden)]
    #[turret(M230, forward = 3.0, side = 0.0)]
    Apache, 
    #[info(
        label = "Arleigh Burke",
        link = "https://en.wikipedia.org/wiki/Arleigh_Burke-class_destroyer"
    )]
    #[entity(Boat, Destroyer, level = 5)]
    #[size(length = 154, width = 20, draft = 9.3, mast = 36.57)]
    #[props(speed = 17, stealth = 0.25)]
    #[sensors(radar, sonar, visual)]
    #[armament(
        Mark54,
        forward = 0.25,
        side = 0.25,
        angle = 0,
        turret = 0,
        symmetrical,
        external
    )]
    #[armament(Mark54, forward = 0.25, angle = 0, turret = 0, external)]
    #[armament(
        Mark54,
        forward = 0.25,
        side = 0.25,
        angle = 0,
        turret = 1,
        symmetrical,
        external
    )]
    #[armament(Mark54, forward = 0.25, angle = 0, turret = 1, external)]
    #[armament(Harpoon, forward = -10.25, side = 5.5, angle = 90, symmetrical, external)]
    #[armament(Harpoon, forward = -11, side = 5.5, angle = 90, symmetrical, external)]
    #[armament(Harpoon, forward = -10.25, side = 5.5, angle = 90, symmetrical, external)]
    #[armament(Harpoon, forward = -11, side = 5.5, angle = 90, symmetrical, external)]
    #[armament(Essm, forward = 39.7, side = 1.5, count = 2, symmetrical, vertical)]
    #[armament(Seahawk, forward = -62, external)]
    #[turret(forward = -15.25, side = 9.4, medium, azimuth_br = 180)]
    #[turret(forward = -15.25, side = -9.4, medium, azimuth_bl = 180)]
    #[turret(Mark12, forward = 51, fast, azimuth_b = 20)]
    #[exhaust(forward = -2)]
    #[exhaust(forward = -18.25)]
    ArleighBurke,
    #[info(
        label = "Bismarck",
        link = "https://en.wikipedia.org/wiki/German_battleship_Bismarck"
    )]
    #[entity(Boat, Battleship, level = 7)]
    #[size(length = 241.6, width = 36, draft = 9.3)]
    #[props(speed = 15.438478)]
    #[sensors(radar, visual)]
    #[armament(Kingfisher, forward = -8.75, side = 5, angle = 90, symmetrical, external)]
    #[turret(_38CmSkc34, forward = 67.9856, slow, azimuth_b = 20)]
    #[turret(_38CmSkc34, forward = 50.672, slow, azimuth_b = 30)]
    #[turret(_38CmSkc34, forward = -55.405, angle = 180, slow, azimuth_b = 30)]
    #[turret(_38CmSkc34, forward = -73.124, angle = 180, slow, azimuth_b = 20)]
    #[exhaust(forward = -1)]
    Bismarck,
    #[info(
        label = "Buyan",
        link = "https://en.wikipedia.org/wiki/Buyan-class_corvette"
    )]
    #[entity(Boat, Corvette, level = 4)]
    #[size(length = 75, width = 11.133, draft = 2.5)]
    #[props(speed = 13.34, stealth = 0.5)]
    #[sensors(radar, sonar, visual)]
    #[armament(Kalibr, forward = -3, side = 0.32, symmetrical, vertical)]
    #[armament(Kalibr, forward = -3.8, side = 0.32, symmetrical, vertical)]
    #[turret(A190, forward = 20.4954, medium, azimuth_b = 40)]
    #[turret(RatepKomar, forward = 15.6236, fast, azimuth_b = 60)]
    #[turret(RatepKomar, forward = -17.8952, angle = 180, fast, azimuth_b = 40)]
    Buyan,
    #[info(
        label = "B-2 Spirit",
        link = "https://en.wikipedia.org/wiki/Northrop_Grumman_B-2_Spirit"
    )]
    #[entity(Boat, Aeroplane, level = 11)]
    #[size(length = 21.0, width = 52.4, draft = 1.0)]
    #[props(speed = 282.944)]
    #[sensors(visual = 1000, radar = 1000)]
    #[armament(Mk82, count = 12, hidden)]
    B2, 
    #[info(
        label = "Clemenceau",
        link = "https://en.wikipedia.org/wiki/Clemenceau-class_aircraft_carrier"
    )]
    #[entity(Boat, Carrier, level = 8)]
    #[size(length = 265, width = 48.6523, draft = 8.6, mast = 61.5)]
    #[props(speed = 16.46223)]
    #[sensors(radar, visual)]
    #[armament(
        SuperEtendard,
        forward = 69.5306,
        side = 4.49494,
        angle = 3,
        count = 3,
        external
    )]
    #[armament(SuperEtendard, forward = -29.9657, side = 12.5451, angle = 8.5, count = 3, external)]
    #[armament(SuperFrelon, forward = 47.67, side = -12.75, angle = 0, external)]
    #[armament(SuperFrelon, forward = -44, side = -13, angle = 0, external)]
    #[turret(_100Mm, forward = 71.4858, side = 16.5069, medium, azimuth_br = 170)]
    #[turret(_100Mm, forward = 59.623, side = 16.5069, medium, azimuth_br = 170)]
    #[turret(_100Mm, forward = -80.893, side = -19.6996, angle = 175, medium, azimuth_br = 175)]
    #[turret(_100Mm, forward = -93.5151, side = -19.6996, angle = 175, medium, azimuth_br = 175)]
    #[turret(Crotale, forward = 67.9671, side = -18.7743, fast)]
    #[turret(Crotale, forward = -82.5578, side = 18.0462, fast)]
    #[exhaust(forward = 4.03893, side = -15.8169)]
    Clemenceau,
    #[info(
        label = "Kaga",
        link = "https://en.wikipedia.org/wiki/Clemenceau-class_aircraft_carrier"
    )]
    #[entity(Boat, Carrier, level = 10)]
    #[size(length = 247.65, width = 32.5, draft = 9.48, mast = 61.5)]
    #[props(speed = 14.4044)]
    #[sensors(radar, visual)]
    #[armament(Type96, forward = 54, external)]
    #[armament(Type96, forward = 36, external)]
    #[armament(Type96, forward = 18, external)]
    #[armament(Type96, forward = 0, external)]
    #[armament(Type96, forward = -18, external)]
    #[armament(Type96, forward = -36, external)]
    #[armament(Type96, forward = -54, external)]
    #[armament(Type96, forward = -72, external)]
    #[armament(Type96, forward = 45, side = 10, symmetrical, external)]
    #[armament(Type96, forward = 27, side = 10, symmetrical, external)]
    #[armament(Type96, forward = 9, side = 10, symmetrical, external)]
    #[armament(Type96, forward = -9, side = 10, symmetrical, external)]
    #[armament(Type96, forward = -27, side = 10, symmetrical, external)]
    #[armament(Type96, forward = -45, side = 10, symmetrical, external)]
    #[armament(Type96, forward = -63, side = 10, symmetrical, external)]
    #[armament(Type96, forward = -81, side = 10, symmetrical, external)]
    #[turret(_200Mm, forward = -37, side = -20, medium)]
    #[turret(_200Mm, forward = -19.5, side = -20, medium)]
    #[turret(_200Mm, forward = -37, side = 19, medium)]
    #[turret(_200Mm, forward = -19.5, side = 19, medium)]
    #[turret(_200Mm, forward = -5, side = 19, medium)]
    #[turret(_200Mm, forward = 69.7, side = 16, medium)]
    #[turret(_200Mm, forward = 70, side = -16.5, medium)]
    #[turret(_200Mm, forward = 84.6, side = -15, medium)]
    #[exhaust(forward = 35, side = -16)]
    Kaga,
    #[info(
        label = "Liaoning",
        link = "https://en.wikipedia.org/wiki/Chinese_aircraft_carrier_Liaoning"
    )]
    #[entity(Boat, Carrier, level = 9)]
    #[size(length = 304.5, width = 67, draft = 11, mast = 80)]
    #[props(speed = 14.918)]
    #[sensors(radar, visual)]
    #[armament(J15, forward = -30, side = -3, angle = 6.3, count = 2, external)]
    #[armament(J15, forward = -10, side = 15, angle = -15, count = 2, external)]
    #[armament(J15, forward = 25, count = 2, side = 3, external)]
    #[armament(J15, forward = -65, count = 2, side = 3, external)]
    #[armament(J15, forward = -125, count = 2, side = 3, external)]
    #[armament(J15, forward = -110, count = 2, side = -15, angle = 60, external)]
    #[armament(Z18, forward = -80, side = 20, angle = 0, external)]
    #[armament(Z18, forward = 30, side = 22.5, angle = 0, external)]
    #[armament(Z18, forward = 40, side = -18, angle = 0, external)]
    #[turret(Hq10, forward = 70, side = 18, medium)]
    #[turret(Hq10, forward = 68, side = -21, medium)]
    #[turret(Hq10, forward = -126, side = -26, medium)]
    #[turret(Hq10, forward = -114, side = 24, medium)]
    #[turret(Type730, forward = -135, side = -23, slow)]
    #[turret(Type730, forward = -125, side = 23, slow)]
    #[exhaust(forward = -44, side = -25)]
    #[exhaust(forward = -48, side = -25)]
    Liaoning,
    #[info(
        label = "CH-47 Chinook",
        link = "https://en.wikipedia.org/wiki/Boeing_CH-47_Chinook"
    )]
    #[entity(Boat, Helicopter, level = 2)]
    #[size(length = 30, width = 18, draft = 0.0)]
    #[props(speed = 82.3111)]
    #[sensors(visual, radar)]
    Chinook, 
    #[info(
        label = "Catalina",
        link = "https://en.wikipedia.org/wiki/Consolidated_PBY_Catalina"
    )]
    #[entity(Boat, Aeroplane, level = 5)]
    #[size(length = 19.47863, width = 32, draft = 1.0)]
    #[props(speed = 87.4556)]
    #[sensors(visual = 600, radar = 500)]
    #[armament(Wz0839, forward = 2, side = 0, hidden)]
    #[turret(_M1919, forward = 7, slow, azimuth_b = 30, symmetrical)]
    #[turret(_M1919, forward = -8, angle = 180, slow, azimuth_b = 40)]
    #[turret(_M1919, forward = -3, slow, azimuth_b = 30, symmetrical)]
    Catalina, 
    #[info(
        label = "Spitfire",
        link = "https://en.wikipedia.org/wiki/Supermarine_Spitfire"
    )]
    #[entity(Boat, Aeroplane, level = 8)]
    #[size(length = 18.24, width = 22.46, draft = 1.0)]
    #[props(speed = 165)]
    #[sensors(visual = 800, radar = 800)]
    #[armament(RP3, forward = 8.5, side = 8, symmetrical)]
    #[turret(_M1919, forward = 7, side = 8, slow, azimuth_b = 150, symmetrical)]
    #[turret(_M1919, forward = 7, slow, azimuth_b = 150)]
    Spitfire, 
    #[info(
        label = "Chengdu J-20",
        link = "https://en.wikipedia.org/wiki/Chengdu_J-20"
    )]
    #[entity(Boat, Aeroplane, level = 10)]
    #[size(length = 21.2, width = 13.01, draft = 1.0)]
    #[props(speed = 333.3)]
    #[armament(Ls6, forward = 2, side = 0, count = 4, hidden)]
    #[armament(Pl12, forward = 2, side = 0, count = 8, hidden)]
    #[sensors(visual = 800, radar = 1300)]
    J20, 
    #[info(
        label = "F-35 Lightning II",
        link = "https://en.wikipedia.org/wiki/Lockheed_Martin_F-35_Lightning_II"
    )]
    #[entity(Boat, Aeroplane, level = 11)]
    #[size(length = 15.7, width = 11, draft = 1.0)]
    #[props(speed = 411.6)]
    #[armament(Jagm, forward = -3, side = 3, symmetrical)]
    #[armament(Jagm, forward = -3, side = 3, symmetrical)]
    #[armament(Jagm, forward = -3, side = 3, symmetrical)]
    #[sensors(visual = 800, radar = 1500)]
    F35, 
    #[info(
        label = "Dreadnought",
        link = "https://en.wikipedia.org/wiki/HMS_Dreadnought_(1906)"
    )]
    #[entity(Boat, Dreadnought, level = 4)]
    #[size(length = 160.9, width = 25.1406, draft = 9)]
    #[props(speed = 10.8)]
    #[sensors(visual)]
    #[armament(Mark18, forward = 45.1688, side = 7.4, angle = 90, symmetrical)]
    #[armament(Mark18, forward = 44.5688, side = 7.5, angle = 90, symmetrical)]
    #[turret(MarkBViii, forward = 37.5478, slow, azimuth_b = 50)]
    #[turret(
        MarkBViii,
        forward = 11.8998,
        side = 8.04308,
        slow,
        symmetrical,
        azimuth_fl = 10,
        azimuth_br = 180
    )]
    #[turret(MarkBViii, forward = -19.2312, angle = 180, slow, azimuth = 40)]
    #[turret(MarkBViii, forward = -45.5178, angle = 180, slow, azimuth_b = 40)]
    #[exhaust(forward = 20.8012)]
    #[exhaust(forward = -5.8322)]
    Dreadnought,
    #[info(
        label = "Dredger",
        link = "https://en.wikipedia.org/wiki/Trailing_suction_hopper_dredger"
    )]
    #[entity(Boat, Dredger, level = 4)]
    #[size(length = 99, width = 16.5, draft = 6.4)]
    #[props(speed = 8)]
    #[sensors(visual)]
    #[armament(Depositor, forward = 7, turret = 0, external)]
    #[turret(forward = 43.75, medium)]
    #[exhaust(forward = -39, side = -0.8)]
    Dredger,
    #[info(
        label = "Drone",
        link = "https://en.wikipedia.org/wiki/Drone"
    )]
    #[entity(Boat, Drone, level = 1)]
    #[size(length = 1.11333, width = 1.40667, draft = 0.0)]
    #[props(speed = 100.0)]
    #[sensors(visual = 1000, radar = 1000, sonar = 1000)]
    Drone,
    #[info(
        label = "España",
        link = "https://en.wikipedia.org/wiki/Espa%C3%B1a-class_battleship"
    )]
    #[entity(Boat, Dreadnought, level = 3)]
    #[size(length = 138.414, width = 24.331, draft = 7.8)]
    #[props(speed = 10.032)]
    #[sensors(visual)]
    #[turret(VickersMkH12In, forward = 34.9562, slow, azimuth_b = 50)]
    #[turret(VickersMkH12In, forward = 13.7379, side = -6.25652, slow, azimuth_fr = 20, azimuth_bl = 180)]
    #[turret(VickersMkH12In, forward = -15.7514, side = 6.73689, angle = 180, slow, azimuth_bl = 180)]
    #[turret(VickersMkH12In, forward = -39.8474, angle = 180, slow, azimuth_b = 45)]
    #[exhaust(forward = -0.822)]
    Espana,
    #[info(
        label = "Ekranoplan",
        link = "https://en.wikipedia.org/wiki/Lun-class_ekranoplan"
    )]
    #[entity(Boat, Ekranoplan, level = 6)]
    #[size(length = 73.8, width = 44.0, draft = 2.5)]
    #[props(speed = 152.79)]
    #[sensors(radar, visual)]
    #[armament(Moskit, forward = 6, side = 1, angle = 0, symmetrical, hidden)]
    #[armament(Moskit, forward = 17, side = 1, angle = 0, symmetrical, hidden)]
    #[armament(Moskit, forward = -5.5, side = 1, angle = 0, symmetrical, hidden)]
    #[turret(_2M3M, forward = 19, side = 1, azimuth_b = 120, symmetrical, fast)]
    Ekranoplan,
    #[info(
        label = "Essex",
        link = "https://en.wikipedia.org/wiki/Essex-class_aircraft_carrier"
    )]
    #[entity(Boat, Carrier, level = 6)]
    #[size(length = 265.8, width = 42.5695, draft = 7, mast = 44.58)]
    #[props(speed = 16.83333)]
    #[sensors(radar, visual)]
    #[armament(Avenger, forward = 16, external)]
    #[armament(Avenger, external)]
    #[armament(Avenger, forward = -16, external)]
    #[armament(Avenger, forward = -32, external)]
    #[armament(Avenger, forward = -48, external)]
    #[armament(Avenger, forward = -64, external)]
    #[turret(Mark12X2, forward = 46.25, side = -12.75, medium, azimuth_b = 20)]
    #[turret(Mark12X2, forward = 38, side = -12.75, medium, azimuth_b = 20)]
    #[turret(Mark12X2, forward = -23.5, side = -12.75, angle = 180, medium, azimuth_b = 20)]
    #[turret(Mark12X2, forward = -31.5, side = -12.75, angle = 180, medium, azimuth_b = 20)]
    #[exhaust(forward = -5.38, side = -12.71)]
    Essex,
    #[info(
        label = "Fairmile D",
        link = "https://en.wikipedia.org/wiki/Fairmile_D_motor_torpedo_boat"
    )]
    #[entity(Boat, Mtb, level = 1)]
    #[size(length = 35, width = 6.35, draft = 1.45)]
    #[props(speed = 15.9477)]
    #[sensors(visual)]
    #[armament(Mark18, forward = -7, side = 2.3, angle = 7.5, symmetrical, external)]
    #[armament(Mark9, forward = 4.5, side = 2.5, angle = 184, symmetrical, external)]
    #[armament(Mark9, forward = 5, side = 2.55, angle = 184, symmetrical, external)]
    #[turret(_6Pounder, forward = 8, fast)]
    #[turret(_6Pounder, forward = -11.5, angle = 180, fast)]
    #[exhaust(forward = 0)]
    FairmileD,
    #[info(
        label = "Fletcher",
        link = "https://en.wikipedia.org/wiki/Fletcher-class_destroyer"
    )]
    #[entity(Boat, Destroyer, level = 4)]
    #[size(length = 114.8, width = 12, draft = 5.3)]
    #[props(speed = 18.777)]
    #[sensors(radar, sonar, visual)]
    #[armament(
        Mark18,
        forward = 0.25,
        side = 1.066,
        angle = 0,
        turret = 0,
        symmetrical,
        external
    )]
    #[armament(
        Mark18,
        forward = 0.25,
        side = 0.533,
        angle = 0,
        turret = 0,
        symmetrical,
        external
    )]
    #[armament(Mark18, forward = 0.25, angle = 0, turret = 0, external)]
    #[armament(
        Mark18,
        forward = 0.25,
        side = 1.066,
        angle = 0,
        turret = 1,
        symmetrical,
        external
    )]
    #[armament(
        Mark18,
        forward = 0.25,
        side = 0.533,
        angle = 0,
        turret = 1,
        symmetrical,
        external
    )]
    #[armament(Mark18, forward = 0.25, angle = 0, turret = 1, external)]
    #[armament(Mark9, forward = -55, angle = 180, external)]
    #[armament(Mark9, forward = -55.5, angle = 180, external)]
    #[armament(Mark9, forward = -56, angle = 180, external)]
    #[armament(Mark9, forward = -56.5, angle = 180, external)]
    #[turret(forward = 2.75, medium, azimuth = 45)]
    #[turret(forward = -13, medium, azimuth = 45)]
    #[turret(Mark12, forward = 37.75, medium, azimuth_b = 20)]
    #[turret(Mark12, forward = 30.24, medium, azimuth_b = 30)]
    #[turret(Mark12, forward = -31.07, angle = 180, medium, azimuth_b = 30)]
    #[turret(Mark12, forward = -38.61, angle = 180, medium, azimuth_b = 20)]
    #[exhaust(forward = 9.5)]
    #[exhaust(forward = -4.5)]
    Fletcher,
    #[info(
        label = "Freccia",
        link = "https://en.wikipedia.org/wiki/Freccia-class_destroyer"
    )]
    #[entity(Boat, Destroyer, level = 3)]
    #[size(length = 96.15, width = 9.3896, draft = 4)]
    #[props(speed = 15.44)]
    #[sensors(radar, visual)]
    #[armament(
        Mark18,
        forward = 0.25,
        side = 0.533,
        angle = 0,
        turret = 0,
        symmetrical,
        external
    )]
    #[armament(Mark18, forward = 0.25, angle = 0, turret = 0, external)]
    #[armament(
        Mark18,
        forward = 0.25,
        side = 0.533,
        angle = 0,
        turret = 1,
        symmetrical,
        external
    )]
    #[armament(Mark18, forward = 0.25, angle = 0, turret = 1, external)]
    #[armament(Mark9, forward = -45.75, side = 1.83, angle = 180, symmetrical, external)]
    #[turret(forward = -9.39937, medium, azimuth = 40)]
    #[turret(forward = -21.8755, medium, azimuth = 40)]
    #[turret(Ansaldo, forward = 28.8815, medium, azimuth_b = 30)]
    #[turret(Ansaldo, forward = -31.6105, angle = 180, medium, azimuth_b = 20)]
    #[exhaust(forward = 7.9804)]
    Freccia,
    #[info(
        label = "Freedom",
        link = "https://en.wikipedia.org/wiki/Freedom-class_littoral_combat_ship"
    )]
    #[entity(Boat, Lcs, level = 6)]
    #[size(length = 115, width = 17.5, draft = 3.9)]
    #[props(speed = 24.1789, stealth = 0.5)]
    #[sensors(radar, sonar, visual)]
    #[armament(Nsm, forward = 26.5436, side = 4.77561, angle = -53.7668, count = 2, symmetrical)]
    #[armament(Nsm, forward = 27.5111, side = 5.51015, angle = -53.7668, count = 2, symmetrical)]
    #[armament(Seahawk, forward = -40, external)]
    #[turret(Bofors57MmMk3, forward = 33, fast, azimuth_b = 35)]
    #[turret(Mark49, forward = -22.5, angle = 180, fast)]
    #[exhaust(forward = 1.4, side = 1.68, symmetrical)]
    Freedom,
    #[info(
        label = "G-5",
        link = "https://en.wikipedia.org/wiki/G-5-class_motor_torpedo_boat"
    )]
    #[entity(Boat, Mtb, level = 1)]
    #[size(length = 18.85, width = 3.5, draft = 0.82)]
    #[props(speed = 27.26557)]
    #[sensors(visual)]
    #[armament(Type53, forward = -7, side = 0.333, angle = 0, symmetrical, external)]
    G5,
    #[info(
        label = "Golf",
        link = "https://en.wikipedia.org/wiki/Golf-class_submarine"
    )]
    #[entity(Boat, Submarine, level = 4)]
    #[size(length = 98.4, width = 8.2, draft = 8.5)]
    #[props(speed = 8.7455, depth = 260)]
    #[sensors(sonar, visual)]
    #[armament(Set65, forward = 41, side = 0.5, angle = 0, symmetrical)]
    #[armament(Set65, forward = 41, side = 0.5, angle = 0, symmetrical)]
    #[armament(Set65, forward = 41, side = 0.5, angle = 0, symmetrical)]
    Golf,
    #[info(
        label = "East Indiaman",
        link = "https://en.wikipedia.org/wiki/East_Indiaman"
    )]
    #[entity(Boat, Pirate, level = 3)]
    #[size(length = 52.8143, width = 13.6162, draft = 5)]
    #[props(speed = 4)]
    #[sensors(visual)]
    #[armament(
        CannonBall,
        forward = 2.72433,
        side = 4.48329,
        angle = 90,
        symmetrical,
        external
    )]
    #[armament(
        CannonBall,
        forward = 7.0183,
        side = 4.53272,
        angle = 89,
        symmetrical,
        external
    )]
    #[armament(CannonBall, forward = -1.48315, side = 4.31076, angle = 91, symmetrical, external)]
    #[armament(
        CannonBall,
        forward = 11.0811,
        side = 4.33021,
        angle = 88,
        symmetrical,
        external
    )]
    #[armament(CannonBall, forward = -9.85305, side = 4.31076, angle = 92, symmetrical, external)]
    Indiaman,
    #[info(
        label = "Iowa",
        link = "https://en.wikipedia.org/wiki/Iowa-class_battleship"
    )]
    #[entity(Boat, Battleship, level = 10)]
    #[size(length = 270.4, width = 32.74, draft = 12, mast = 38.9)]
    #[props(speed = 16.977)]
    #[sensors(radar, visual)]
    #[armament(Tomahawk, forward = -13.45, side = 10.748, angle = -90, count = 2, symmetrical, hidden)]
    #[armament(Tomahawk, forward = -17.08, side = 10.748, angle = -90, count = 2, symmetrical, hidden)]
    #[armament(Tomahawk, forward = -41.02, side = 4.45, angle = 30, count = 2, symmetrical, hidden)]
    #[armament(Tomahawk, forward = -46.9846, side = 4.45086, angle = 30, count = 2, symmetrical, hidden)]
    #[armament(Seahawk, forward = -121, external)]
    #[armament(Seahawk, forward = -109, side = -8, angle = -15, symmetrical, external)]
    #[turret(Mark7, forward = 59.62, slow, azimuth_b = 20)]
    #[turret(Mark7, forward = 38.25, slow, azimuth_b = 30)]
    #[turret(Mark7, forward = -65.56, angle = 180, slow, azimuth_b = 30)]
    #[exhaust(forward = -4.41)]
    #[exhaust(forward = -30.58)]
    Iowa,
    #[info(
        label = "Kirov",
        link = "https://en.wikipedia.org/wiki/Kirov-class_battlecruiser"
    )]
    #[entity(Boat, Cruiser, level = 9)]
    #[size(length = 252, width = 28.793, draft = 9.1, mast = 49.71)]
    #[props(speed = 16.46223)]
    #[sensors(radar, sonar, visual)]
    #[armament(Set65, forward = -50.5471, side = 10, angle = 90, symmetrical)]
    #[armament(Set65, forward = -51.0471, side = 10, angle = 90, symmetrical)]
    #[armament(Set65, forward = -51.5471, side = 10, angle = 90, symmetrical)]
    #[armament(Set65, forward = -52.0471, side = 10, angle = 90, symmetrical)]
    #[armament(P700, forward = 41, side = 3.5, count = 4, symmetrical, hidden)]
    #[armament(S300, forward = 61.2, side = 4.7, count = 3, symmetrical, vertical)]
    #[armament(Ka25, forward = -112.4, external)]
    #[turret(Ak130, forward = -66.6097, angle = 180, medium, azimuth_b = 30)]
    #[turret(Ak130, forward = -79.1108, angle = 180, medium, azimuth_b = 20)]
    #[exhaust(forward = -19)]
    Kirov,
    #[info(
        label = "Kolkata",
        link = "https://en.wikipedia.org/wiki/Kolkata-class_destroyer"
    )]
    #[entity(Boat, Destroyer, level = 6)]
    #[size(length = 163, width = 17.4, draft = 6.5)]
    #[props(speed = 15.43334, stealth = 0.5)]
    #[sensors(radar, sonar, visual)]
    #[armament(
        Set65,
        forward = 0.25,
        side = 0.25,
        angle = 0,
        turret = 0,
        symmetrical,
        external
    )]
    #[armament(
        Set65,
        forward = 0.25,
        side = 0.25,
        angle = 0,
        turret = 1,
        symmetrical,
        external
    )]
    #[armament(BrahMos, forward = 43.4, side = 1.4, count = 3, symmetrical, vertical)]
    #[armament(Barak8, forward = 37.5, side = 2, symmetrical, vertical)]
    #[armament(Barak8, forward = -36.3, side = 1.5, symmetrical, vertical)]
    #[armament(Ka25, forward = -70, external)]
    #[turret(forward = -2.5, side = -2.5, angle = -90, medium, azimuth_b = 155)]
    #[turret(forward = -5.3, side = 2.5, angle = 90, medium, azimuth_b = 155)]
    #[turret(OtoMelara76Mm, forward = 54, fast, azimuth_b = 20)]
    #[exhaust(forward = 3.74)]
    #[exhaust(forward = -24.21)]
    Kolkata,
    #[info(
        label = "Komar",
        link = "https://en.wikipedia.org/wiki/Komar-class_missile_boat"
    )]
    #[entity(Boat, Mtb, level = 1)]
    #[size(length = 25.4, width = 6.24, draft = 1.24)]
    #[props(speed = 22.6)]
    #[sensors(radar, visual)]
    #[armament(Type53, forward = -0.5, side = 2.3, angle = 5.2, symmetrical, external)]
    #[armament(Mark9, forward = -11.5, side = 1.2, angle = 182, symmetrical, external)]
    #[armament(Mark9, forward = -12, side = 1.2, angle = 182, symmetrical, external)]
    #[turret(_2M3M, forward = 3.4, side = 0.8, angle = 0, fast)]
    #[turret(_2M3M, forward = -8.5, angle = 180, fast)]
    Komar,
    #[info(
        label = "Leander",
        link = "https://en.wikipedia.org/wiki/Leander-class_cruiser_(1931)"
    )]
    #[entity(Boat, Cruiser, level = 5)]
    #[size(length = 169.1, width = 17.1, draft = 5.8)]
    #[props(speed = 16.71945)]
    #[sensors(radar, visual)]
    #[armament(
        Mark18,
        forward = 0.25,
        side = 0.26,
        angle = 0,
        turret = 0,
        symmetrical,
        external
    )]
    #[armament(
        Mark18,
        forward = 0.25,
        side = 0.78,
        angle = 0,
        turret = 0,
        symmetrical,
        external
    )]
    #[armament(
        Mark18,
        forward = 0.25,
        side = 0.26,
        angle = 0,
        turret = 1,
        symmetrical,
        external
    )]
    #[armament(
        Mark18,
        forward = 0.25,
        side = 0.78,
        angle = 0,
        turret = 1,
        symmetrical,
        external
    )]
    #[turret(forward = -3.41018, side = 6.52922, angle = 180, medium, azimuth_fl = 180)]
    #[turret(forward = -3.41018, side = -6.52922, angle = 180, medium, azimuth_fr = 180)]
    #[turret(Bl6MkXxiii, forward = 52.7746, medium, azimuth_b = 20)]
    #[turret(Bl6MkXxiii, forward = 43.2429, medium, azimuth_b = 30)]
    #[turret(Bl6MkXxiii, forward = -45.3247, angle = 180, medium, azimuth_b = 30)]
    #[turret(Bl6MkXxiii, forward = -56.3283, angle = 180, medium, azimuth_b = 20)]
    #[exhaust(forward = 7)]
    Leander,
    #[info(
        label = "Lublin",
        link = "https://en.wikipedia.org/wiki/Lublin-class_minelayer-landing_ship"
    )]
    #[entity(Boat, Minelayer, level = 3)]
    #[size(length = 95.8, width = 10.8, draft = 2.38)]
    #[props(speed = 8.5)]
    #[sensors(radar, visual)]
    #[armament(Wz0839, forward = -40, side = 1.75, symmetrical, external)]
    #[armament(Wz0839, forward = -41, side = 1.75, symmetrical, external)]
    #[armament(Wz0839, forward = -42, side = 1.75, symmetrical, external)]
    Lublin,
    #[info(
        label = "Momi",
        link = "https://en.wikipedia.org/wiki/Momi-class_destroyer"
    )]
    #[entity(Boat, Destroyer, level = 2)]
    #[size(length = 85.3, width = 7.9, draft = 2.4)]
    #[props(speed = 18.52)]
    #[sensors(radar, visual)]
    #[armament(
        Mark18,
        forward = 0.25,
        side = 0.3,
        angle = 0,
        turret = 0,
        symmetrical,
        external
    )]
    #[armament(
        Mark18,
        forward = 0.25,
        side = 0.3,
        angle = 0,
        turret = 1,
        symmetrical,
        external
    )]
    #[armament(Mark9, forward = -42, side = 1.4, angle = 180, symmetrical, external)]
    #[armament(Mark9, forward = -41.5, side = 1.4, angle = 180, symmetrical, external)]
    #[turret(forward = 22.15, medium, azimuth = 45)]
    #[turret(forward = -13.85, medium, azimuth = 45)]
    #[turret(Mark12, forward = 30, medium, azimuth_b = 20)]
    #[turret(Mark12, forward = 1.5, angle = 180, medium, azimuth = 30)]
    #[turret(Mark12, forward = -22.5, angle = 180, medium, azimuth_b = 20)]
    #[exhaust(forward = 7.84)]
    #[exhaust(forward = -3.09)]
    Momi,
    #[info(
        label = "Montana",
        link = "https://en.wikipedia.org/wiki/Montana-class_battleship"
    )]
    #[entity(Boat, Battleship, level = 8)]
    #[size(length = 280.8, width = 36.93, draft = 10.97, mast = 36.82)]
    #[props(speed = 14.404)]
    #[sensors(radar, visual)]
    #[armament(Kingfisher, forward = -122, side = 8.5, angle = 17.5, symmetrical, external)]
    #[turret(Mark7, forward = 74.62, slow, azimuth_b = 20)]
    #[turret(Mark7, forward = 52.5, slow, azimuth_b = 30)]
    #[turret(Mark7, forward = -47.9, angle = 180, slow, azimuth_b = 30)]
    #[turret(Mark7, forward = -69.49, angle = 180, slow, azimuth_b = 20)]
    #[exhaust(forward = 10)]
    #[exhaust(forward = -14.5)]
    Montana,
    #[info(
        label = "Moskva",
        link = "https://en.wikipedia.org/wiki/Moskva-class_helicopter_carrier"
    )]
    #[entity(Boat, Carrier, level = 7)]
    #[size(length = 189, width = 34, draft = 7.84, mast = 48.04)]
    #[props(speed = 14.66167)]
    #[sensors(radar, sonar, visual)]
    #[armament(Ka25, forward = -23.535, side = 7.74318, external)]
    #[armament(Ka25, forward = -38.6508, side = -8.0173, external)]
    #[armament(Ka25, forward = -64.7966, side = 7.39509, external)]
    #[armament(Ka25, forward = -84.7862, side = -2.81806, external)]
    #[armament(Set65, forward = -3.02179, side = 10.358, angle = 90, symmetrical)]
    #[armament(Set65, forward = -3.62179, side = 10.358, angle = 90, symmetrical)]
    #[armament(Set65, forward = -4.22179, side = 10.358, angle = 90, symmetrical)]
    #[armament(Set65, forward = -4.82179, side = 10.358, angle = 90, symmetrical)]
    #[armament(Set65, forward = -5.42179, side = 10.358, angle = 90, symmetrical)]
    #[turret(Shtorm, forward = 50.3038, medium)]
    #[turret(Shtorm, forward = 28.689, medium, azimuth_b = 30)]
    #[exhaust(forward = -13.35)]
    Moskva,
    #[info(
        label = "Oberon",
        link = "https://en.wikipedia.org/wiki/Oberon-class_submarine"
    )]
    #[entity(Boat, Submarine, level = 3)]
    #[size(length = 90, width = 8.25, draft = 5.5)]
    #[props(speed = 8.9408, depth = 200)]
    #[sensors(sonar, visual)]
    #[armament(Mark18, forward = 40, side = 0.5, angle = 2, count = 3, symmetrical)]
    #[armament(Mark18, forward = -41.4, side = 0.3, angle = 180, symmetrical)]
    Oberon,
    #[info(
        label = "Ohio",
        link = "https://en.wikipedia.org/wiki/Ohio-class_submarine"
    )]
    #[entity(Boat, Submarine, level = 7)]
    #[size(length = 170, width = 13, draft = 10.8)]
    #[props(speed = 12.8611, depth = 400)]
    #[sensors(radar, sonar, visual)]
    #[armament(Mark48, forward = 72, side = 5, angle = 0, symmetrical)]
    #[armament(Mark48, forward = 72, side = 5, angle = 0, symmetrical)]
    #[armament(Mk70, forward = 72, side = 5, angle = 0, hidden)]
    #[armament(Tomahawk, forward = 30.3, side = 2, angle = 0, symmetrical, vertical)]
    #[armament(Tomahawk, forward = 23.7, side = 2, angle = 0, symmetrical, vertical)]
    #[armament(Tomahawk, forward = 17.2, side = 2, angle = 0, symmetrical, vertical)]
    #[armament(Tomahawk, forward = 10.75, side = 2, angle = 0, symmetrical, vertical)]
    #[armament(Tomahawk, forward = 4.25, side = 2, angle = 0, symmetrical, vertical)]
    Ohio,
    #[info(
        label = "Olympias",
        link = "https://en.wikipedia.org/wiki/Olympias_%28trireme%29"
    )]
    #[entity(Boat, Ram, level = 1)]
    #[size(length = 36.9, width = 5.5, draft = 1.25)]
    #[props(speed = 16, ram_damage = 3)]
    #[sensors(visual)]
    Olympias,
    #[info(
        label = "Osa",
        link = "https://en.wikipedia.org/wiki/Osa-class_missile_boat"
    )]
    #[entity(Boat, Mtb, level = 3)]
    #[size(length = 38.6, width = 7.64, draft = 1.73)]
    #[props(speed = 21.6067)]
    #[sensors(radar, visual)]
    #[armament(P15, forward = -1.75, side = 2.5, angle = 2, symmetrical)]
    #[armament(P15, forward = -12, side = 2.5, angle = 2, symmetrical)]
    #[turret(_2M3M, forward = 10, angle = 0, fast)]
    #[turret(_2M3M, forward = -16.5, angle = 180, fast)]
    Osa,
    #[info(
        label = "PT-34",
        link = "https://en.wikipedia.org/wiki/Patrol_torpedo_boat_PT-34"
    )]
    #[entity(Boat, Mtb, level = 1)]
    #[size(length = 23, width = 6.07, draft = 1.37)]
    #[props(speed = 21.09)]
    #[sensors(visual)]
    #[armament(Mark18, side = 2.5, angle = 4.5, symmetrical, external)]
    #[armament(Mark18, forward = -8, side = 1.8, angle = 4.5, symmetrical, external)]
    Pt34,
    #[info(
        label = "Seawolf",
        link = "https://en.wikipedia.org/wiki/Seawolf-class_submarine"
    )]
    #[entity(Boat, Submarine, level = 8)]
    #[size(length = 108, width = 17.6133, draft = 11)]
    #[props(speed = 18.00556, depth = 400, stealth = 0.5)]
    #[sensors(radar, sonar, visual)]
    #[armament(
        Mark48,
        forward = 37.7849,
        side = 4.73435,
        angle = 0,
        count = 4,
        symmetrical
    )]
    #[armament(
        Mk70,
        forward = 37.7849,
        side = 4.73435,
        angle = 0,
        symmetrical,
        hidden
    )]
    Seawolf,
    #[info(
        label = "Skipjack",
        link = "https://en.wikipedia.org/wiki/Skipjack-class_submarine"
    )]
    #[entity(Boat, Submarine, level = 5)]
    #[size(length = 76.71, width = 9.65, draft = 7.66, mast = 10.40)]
    #[props(speed = 16.976667, depth = 210)]
    #[sensors(radar, sonar, visual)]
    #[armament(Mark48, forward = 33.75, side = 0.7, angle = 0, symmetrical)]
    #[armament(Mark48, forward = 33.75, side = 0.7, angle = 0, symmetrical)]
    #[armament(Mk70, forward = 33.75, side = 0.7, angle = 0, hidden)]
    #[armament(Harpoon, forward = 34, angle = 0, symmetrical)]
    Skipjack,
    #[info(
        label = "Skjold",
        link = "https://en.wikipedia.org/wiki/Skjold-class_corvette"
    )]
    #[entity(Boat, Corvette, level = 7)]
    #[size(length = 47.5, width = 13.73, draft = 1)]
    #[props(speed = 30.867, stealth = 0.75)]
    #[sensors(radar, sonar, visual)]
    #[armament(Nsm, forward = -19.0286, side = -1.96027, angle = -23.7601, count = 2, symmetrical)]
    #[armament(Nsm, forward = -19.3748, side = -2.88731, angle = -23.7601, count = 2, symmetrical)]
    #[armament(Mistral, forward = -6.08214, side = -4.51251, vertical, count = 3, symmetrical)]
    #[turret(OtoMelara76Mm, forward = 6.02709, fast, azimuth_b = 35)]
    Skjold,
    #[info(
        label = "M4 Sherman",
        link = "https://en.wikipedia.org/wiki/M4_Sherman"
    )]
    #[entity(Boat, Tank, level = 4)]
    #[size(length = 5.89, width = 2.87597, draft = 1.0)]
    #[props(speed = 9.38784, ram_damage = 3)]
    #[sensors(visual = 600, radar = 600)]
    #[turret(ShermanTurret, forward = -0.028703, fast)]
    Sherman,
    #[info(
        label = "Imperial II-Class Star Destroyer",
        link = "https://starwars.fandom.com/wiki/Imperial_II-class_Star_Destroyer"
    )]
    #[entity(Boat, Starship, level = 12)]
    #[size(length = 1600, width = 878, draft = 0.0)]
    #[props(speed = 270.833)]
    #[sensors(visual, radar)]
    #[armament(TieFighter, forward = 0.0, side = 0.0, angle = 0.0, count = 12, hidden)]
    #[turret(Turbolaser, forward = 130.8086, side = -215.0364, symmetrical)]
    #[turret(Turbolaser, forward = 72.7645, side = -232.6973, symmetrical)]
    #[turret(Turbolaser, forward = 19.7676, side = -249.5172, symmetrical)]
    #[turret(Turbolaser, forward = -259.5174, side = -335.2989, symmetrical)]
    StarDestroyer, //"Star Wars: Imperial II Star Destroyer" (https://skfb.ly/LuuA) by Daniel is licensed under Creative Commons Attribution (http://creativecommons.org/licenses/by/4.0/).
    #[info(
        label = "Oil Tanker",
        link = "https://en.wikipedia.org/wiki/Oil_tanker"
    )]
    #[entity(Boat, Tanker, level = 5)]
    #[size(length = 179, width = 30.94, draft = 11.6)]
    #[props(speed = 8.333333)]
    #[sensors(visual)]
    #[exhaust(forward = -77)]
    Tanker,
    #[info(
        label = "Terry Fox",
        link = "https://en.wikipedia.org/wiki/CCGS_Terry_Fox"
    )]
    #[entity(Boat, Icebreaker, level = 6)]
    #[size(length = 88, width = 17.7031, draft = 8.3)]
    #[props(speed = 8.231111, ram_damage = 2.5)]
    #[sensors(radar, visual)]
    #[exhaust(forward = 7.308, side = 4.531, symmetrical)]
    TerryFox,
    #[info(
        label = "Town",
        link = "https://en.wikipedia.org/wiki/Town-class_cruiser_(1936)"
    )]
    #[entity(Boat, Cruiser, level = 6)]
    #[size(length = 180.3, width = 20.77676, draft = 6.28)]
    #[props(speed = 16.59084)]
    #[sensors(radar, visual)]
    #[armament(Mark18, forward = 0.25, angle = 0, turret = 0, external)]
    #[armament(
        Mark18,
        forward = 0.25,
        side = 0.52,
        angle = 0,
        turret = 0,
        symmetrical,
        external
    )]
    #[armament(Mark18, forward = 0.25, angle = 0, turret = 1, external)]
    #[armament(
        Mark18,
        forward = 0.25,
        side = 0.52,
        angle = 0,
        turret = 1,
        symmetrical,
        external
    )]
    #[armament(Kingfisher, forward = 4.82098, external)]
    #[turret(forward = -20.2181, side = 8.41364, medium, azimuth_br = 180)]
    #[turret(forward = -20.2181, side = -8.41364, medium, azimuth_bl = 180)]
    #[turret(Bl6MkXxiiiX3, forward = 59.4418, medium, azimuth_b = 20)]
    #[turret(Bl6MkXxiiiX3, forward = 48.659, medium, azimuth_b = 30)]
    #[turret(Bl6MkXxiiiX3, forward = -47.9432, angle = 180, medium, azimuth_b = 30)]
    #[turret(Bl6MkXxiiiX3, forward = -59.1084, angle = 180, medium, azimuth_b = 20)]
    #[exhaust(forward = 17)]
    #[exhaust(forward = -8)]
    Town,
    #[info(
        label = "Type 055",
        link = "https://en.wikipedia.org/wiki/Type_055_destroyer"
    )]
    #[entity(Boat, Destroyer, level = 7)]
    #[size(length = 180, width = 20, draft = 9.5, mast = 36.28)]
    #[props(speed = 15.434, stealth = 0.75)]
    #[sensors(radar, sonar, visual)]
    #[armament(Yj18, forward = 41.4, side = 2, count = 4, symmetrical, vertical)]
    #[armament(_82R, forward = -39.8359, side = 8, angle = 90, symmetrical)]
    #[armament(_82R, forward = -40.4359, side = 8, angle = 90, symmetrical)]
    #[armament(_82R, forward = -41.0359, side = 8, angle = 90, symmetrical)]
    #[armament(Hq9, forward = 46.5, side = 2, symmetrical, vertical)]
    #[armament(Hq9, forward = -33.8354, side = 2, count = 2, symmetrical, vertical)]
    #[armament(Harbin, forward = -79.8795, external)]
    #[turret(Hpj38, forward = 58.9931, fast, azimuth_b = 15)]
    #[exhaust(forward = -7.34, side = 1.45, symmetrical)]
    #[exhaust(forward = -17.34, side = 1.45, symmetrical)]
    Type055,
    #[info(
        label = "Type VII C",
        link = "https://en.wikipedia.org/wiki/Type_VII_submarine"
    )]
    #[entity(Boat, Submarine, level = 2)]
    #[size(length = 67.1, width = 6.2, draft = 4.74)]
    #[props(speed = 9.06, depth = 180)]
    #[sensors(sonar, visual)]
    #[armament(Mark18, forward = 26, side = 0.333, angle = 2, symmetrical)]
    #[armament(Mark18, forward = 25, side = 0.666, angle = 2, symmetrical)]
    #[armament(Mark18, forward = -30, angle = 180)]
    #[turret(_88CmSkc35, forward = -4.35, angle = 180, medium, azimuth_b = 20)]
    TypeViic,
    #[info(
        label = "Ticonderoga",
        link = "https://en.wikipedia.org/wiki/Ticonderoga-class_cruiser"
    )]
    #[entity(Boat, Cruiser, level = 8)]
    #[size(length = 173, width = 16.8, draft = 10.2)]
    #[props(speed = 16.71944)]
    #[sensors(radar, visual)]
    #[armament(Seahawk, forward = -42, count = 2, external)]
    #[armament(Harpoon, forward = 43, count = 4, side = 0, vertical)]
    #[armament(Harpoon, forward = -62, count = 4, side = 0, vertical)]
    #[armament(Tomahawk, forward = 43, count = 6, side = 0, vertical)]
    #[armament(Tomahawk, forward = -62, count = 6, side = 0, vertical)]
    #[armament(Asroc, forward = 43, side = 0, count = 2, vertical)]
    #[armament(Mk3, forward = -85, side = 0, angle = -180, hidden)]
    Ticonderoga, 
    #[info(
        label = "Titanic",
        link = "https://en.wikipedia.org/wiki/Titanic"
    )]
    #[entity(Boat, Passenger, level = 7)]
    #[size(length = 269.1, width = 28.2, draft = 10.5)]
    #[props(speed = 11.8332)]
    #[sensors(radar, visual)]
    #[exhaust(forward = -14)]
    #[exhaust(forward = -18)]
    #[exhaust(forward = 17)]
    #[exhaust(forward = 21)]
    #[exhaust(forward = 54)]
    #[exhaust(forward = 57)]
    Titanic,
    #[info(
        label = "UAP",
        link = "https://en.wikipedia.org/wiki/Pentagon_UFO_videos"
    )]
    #[entity(Boat, Drone, level = 1)]
    #[size(length = 12, width = 7.4165, draft = 0.0)]
    #[props(speed = 1000.0, stealth = 0.95)]
    #[sensors(visual = 750, radar = 750, sonar = 750)]
    Uap,
    #[info(
        label = "Nexar Vindicator",
        link = "http://astroflux.org/wiki/index.php/Nexar_Vindicator"
    )]
    #[entity(Boat, Aeroplane, level = 12)]
    #[size(length = 28.8, width = 29.88, draft = 1.0)]
    #[props(speed = 350.0)]
    #[sensors(visual = 1000, radar = 1000)]
    #[armament(VBlaster, forward = 5.0, count = 8, hidden)]
    #[armament(VMissiles, forward = 5.0, count = 8, hidden)]
    #[armament(VProjector, forward = 5.0, count = 1, hidden)]
    Vindicator, 
    #[info(
        label = "Visby",
        link = "https://en.wikipedia.org/wiki/Visby-class_corvette"
    )]
    #[entity(Boat, Corvette, level = 5)]
    #[size(length = 72.7, width = 10.4, draft = 2.4, mast = 17.03)]
    #[props(speed = 18.00556, stealth = 0.75)]
    #[sensors(radar, sonar, visual)]
    #[armament(
        Torped45,
        forward = 0.25,
        side = 0.15,
        angle = 0,
        turret = 0,
        symmetrical,
        external
    )]
    #[armament(
        Torped45,
        forward = 0.25,
        side = 0.15,
        angle = 0,
        turret = 1,
        symmetrical,
        external
    )]
    #[armament(Rbs15, forward = -2.25, side = 3.5, angle = 90, symmetrical, external)]
    #[armament(Rbs15, forward = -3, side = 3.5, angle = 90, symmetrical, external)]
    #[armament(Rbs15, forward = -2.25, side = 3.5, angle = 90, symmetrical, external)]
    #[armament(Rbs15, forward = -3, side = 3.5, angle = 90, symmetrical, external)]
    #[armament(Seahawk, forward = -23, external)]
    #[turret(forward = -22, side = 4.5, medium, azimuth_br = 180)]
    #[turret(forward = -22, side = -4.5, medium, azimuth_bl = 180)]
    #[turret(Bofors57MmMk3, forward = 20, fast, azimuth_b = 30)]
    Visby,
    #[info(
        label = "Virginia",
        link = "https://en.wikipedia.org/wiki/Virginia-class_submarine"
    )]
    #[entity(Boat, Submarine, level = 10)]
    #[size(length = 115, width = 10, draft = 11)]
    #[props(speed = 18.0056, depth = 490, stealth = 0.65)]
    #[sensors(radar, sonar, visual)]
    #[armament(
        Mark48,
        forward = 37.7849,
        side = 4.73435,
        angle = 0,
        count = 4,
        symmetrical
    )]
    #[armament(
        Mk3,
        forward = 37.7849,
        side = 4.73435,
        angle = 0,
        symmetrical,
        hidden
    )]
    #[armament(Tomahawk, forward = 30.3, side = 2, angle = 0, symmetrical, vertical)]
    #[armament(Tomahawk, forward = 23.7, side = 2, angle = 0, symmetrical, vertical)]
    #[armament(Tomahawk, forward = 17.2, side = 2, angle = 0, symmetrical, vertical)]
    Virginia,
    #[info(
        label = "T-65B X-wing starfighter",
        link = "https://starwars.fandom.com/wiki/T-65B_X-wing_starfighter"
    )]
    #[entity(Boat, Aeroplane, level = 9)]
    #[size(length = 13.4, width = 11.76, draft = 1.2)]
    #[props(speed = 291.6667)]
    #[sensors(visual = 800, radar = 1000)]
    #[armament(Blaster, forward = 2, side = 5.6, count = 4, hidden, symmetrical)]
    Xwing, 
    #[info(
        label = "Yamato",
        link = "https://en.wikipedia.org/wiki/Japanese_battleship_Yamato"
    )]
    #[entity(Boat, Battleship, level = 9)]
    #[size(length = 263, width = 40.0664, draft = 11, mast = 43.46)]
    #[props(speed = 13.89, torpedo_resistance = 0.2)]
    #[sensors(radar, visual)]
    #[armament(E4N, forward = -115.239, side = 9.9026, angle = 174, symmetrical, external)]
    #[armament(E4N, forward = -100.891, side = 11.1675, angle = 186.81, symmetrical, external)]
    #[turret(_45Type94, forward = 51.655, slow, azimuth_b = 30)]
    #[turret(_45Type94, forward = 29.2646, slow, azimuth_b = 40)]
    #[turret(_45Type94, forward = -64.996, angle = 180, slow, azimuth_b = 40)]
    #[exhaust(forward = -24.7)]
    Yamato,
    #[info(
        label = "Yasen",
        link = "https://en.wikipedia.org/wiki/Yasen-class_submarine"
    )]
    #[entity(Boat, Submarine, level = 9)]
    #[size(length = 130, width = 19.804688, draft = 10)]
    #[props(speed = 18.00556, depth = 450)]
    #[sensors(radar, sonar, visual)]
    #[armament(Set65, forward = 41, side = 5.75, angle = 2, count = 3, symmetrical)]
    #[armament(Rpk6, forward = 41, side = 5.75, angle = 2, count = 2, symmetrical)]
    #[armament(BrahMos, forward = -4.5, side = 2, symmetrical, vertical)]
    #[armament(BrahMos, forward = -7, side = 2, symmetrical, vertical)]
    #[armament(Igla, forward = 29.19, count = 4, vertical)]
    #[armament(Brosok, forward = 43, side = 3, angle = 0, symmetrical)]
    #[armament(Brosok, forward = -16.5, side = 1.5, angle = -180, symmetrical)]
    Yasen,
    #[info(label = "Zubr", link = "https://en.wikipedia.org/wiki/Zubr-class_LCAC")]
    #[entity(Boat, Hovercraft, level = 2)]
    #[size(length = 57, width = 21.152344, draft = 1.6)]
    #[props(speed = 28.29446)]
    #[sensors(radar, visual)]
    #[turret(Ogon, forward = 15.2, fast)]
    #[turret(_2M3M, forward = 10, side = 6.25, angle = 0, fast, symmetrical)]
    #[exhaust(forward = -22.5)]
    #[exhaust(forward = -22.5, side = 6.91, symmetrical)]
    Zubr,
    #[info(label = "Landing Ship, Tank", link = "https://en.wikipedia.org/wiki/Landing_Ship,_Tank")]
    #[entity(Boat, LandingShip, level = 4)]
    #[size(length = 33.33, width = 5.66, draft = 1.0)]
    #[props(speed = 5.65889)]
    #[sensors(radar, visual)]
    #[turret(_2M3M, forward = 10, angle = 0, fast)]
    Lst,
    #[info(label = "Zudredger", link = "https://en.wikipedia.org/wiki/Zubr-class_LCAC")]
    #[entity(Boat, Hovercraft, level = 11)]
    #[size(length = 57, width = 21.152344, draft = 1.6)]
    #[props(speed = 38.29446)]
    #[sensors(radar, visual)]
    #[armament(Depositor, forward = 7, turret = 0, external)]
    #[armament(Shovel, forward = 7, turret = 1, external)]
    #[turret(side = 3, forward = 15, medium)]
    #[turret(side = -3, forward = 15, medium)]
    #[exhaust(forward = -22.5)]
    #[exhaust(forward = -22.5, side = 6.91, symmetrical)]
    Zudredger,
    #[info(
        label = "Zumwalt",
        link = "https://en.wikipedia.org/wiki/Zumwalt-class_destroyer"
    )]
    #[entity(Boat, Destroyer, level = 8)]
    #[size(length = 190, width = 24.6, draft = 13.09, mast = 28.67)]
    #[props(speed = 15.434, stealth = 0.75, ram_damage = 1.5)]
    #[sensors(radar, sonar, visual)]
    #[armament(Tomahawk, forward = 16, side = 9, count = 2, symmetrical, vertical)]
    #[armament(Tomahawk, forward = -51.5, side = 9, count = 2, symmetrical, vertical)]
    #[armament(Asroc, forward = 39.5, side = 5.5, count = 2, symmetrical, vertical)]
    #[armament(Essm, forward = 35, side = 6, count = 2, symmetrical, vertical)]
    #[armament(Essm, forward = -56, side = 9, count = 2, symmetrical, vertical)]
    #[armament(Seahawk, forward = -65, external)]
    #[turret(Mark51, forward = 49.5963, medium, azimuth_b = 20)]
    #[turret(Mark51, forward = 25.2885, medium, azimuth_b = 30)]
    #[exhaust(forward = -0.09, side = 0.1)]
    #[exhaust(forward = -18.58, side = -0.72)]
    Zumwalt,
    #[info(label = "Barrel")]
    #[entity(Collectible, Score, level = 1)]
    #[size(length = 2.72, width = 1.785)]
    #[props(speed = 20, reload = 0, lifespan = 60)]
    Barrel,
    #[info(label = "Coin")]
    #[entity(Collectible, Score, level = 5)]
    #[size(length = 3, width = 3)]
    #[props(speed = 15, reload = 0, lifespan = 120)]
    Coin,
    #[info(label = "Crate")]
    #[entity(Collectible, Score, level = 1)]
    #[size(length = 2, width = 2)]
    #[props(speed = 20, reload = 2, lifespan = 60)]
    Crate,
    #[info(label = "Scrap")]
    #[entity(Collectible, Score, level = 2)]
    #[size(length = 3, width = 3)]
    #[props(speed = 15, reload = 1, lifespan = 80)]
    Scrap,
    #[info(label = "Brosok", link = "http://cmano-db.com/weapon/2176/")]
    #[entity(Decoy, Sonar, level = 4)]
    #[size(length = 1.5, width = 0.28125)]
    #[props(speed = 12, lifespan = 15)]
    Brosok,
    #[info(
        label = "MOSS",
        link = "https://en.wikipedia.org/wiki/Mobile_submarine_simulator"
    )]
    #[entity(Decoy, Sonar, level = 2)]
    #[size(length = 2.075, width = 0.29)]
    #[props(speed = 10, lifespan = 15)]
    Mk70,
    #[info(
        label = "Mk3 Sonobuoy Countermeasure",
        link = "http://cmano-db.com/pdf/weapon/136/"
    )]
    #[entity(Decoy, Sonar, level = 5)]
    #[size(length = 2.69, width = 0.159)]
    #[props(speed = 15, lifespan = 30)]
    Mk3,
    #[info(label = "P-270 Moskit", link = "https://en.wikipedia.org/wiki/P-270_Moskit")]
    #[entity(Weapon, Missile, level = 9)]
    #[size(length = 9.745, width = 0.8)]
    #[props(speed = 1027.778, range = 130000)]
    #[sensors(radar)]
    Moskit,
    #[info(label = "AGM-179 JAGM", link = "https://en.wikipedia.org/wiki/AGM-179_JAGM")]
    #[entity(Weapon, Missile, level = 15)]
    #[size(length = 1.8, width = 0.18)]
    #[props(speed = 1000, range = 8000)]
    #[sensors(radar)]
    Jagm,
    #[info(label = "Acacia")]
    #[entity(Obstacle, Tree)]
    #[size(length = 10, width = 10)]
    Acacia,
    #[info(label = "Average Tree")]
    #[entity(Obstacle, Tree)]
    #[size(length = 12, width = 12)]
    AverageTree,
    #[info(label = "Palm Tree")]
    #[entity(Obstacle, Tree)]
    #[size(length = 14, width = 14)]
    Palm,
    #[info(label = "HQ")]
    #[entity(Obstacle, Structure)]
    #[size(length = 90, width = 90)]
    #[props(lifespan = 600)]
    Hq,
    #[info(label = "Oil Platform")]
    #[entity(Obstacle, Structure)]
    #[size(length = 90, width = 90)]
    #[props(lifespan = 600)]
    #[exhaust(forward = 7, side = 21)]
    #[exhaust(forward = -23, side = 21)]
    OilPlatform,
    #[info(label = "Super Oil Platform")]
    #[entity(Obstacle, Structure)]
    #[size(length = 90, width = 90)]
    #[props(lifespan = 600)]
    #[exhaust(forward = 7, side = 21)]
    #[exhaust(forward = -23, side = 21)]
    SuperOilPlatform,
    #[info(label = "M230 Chain Gun")]
    #[entity(Turret, Gun)]
    #[size(length = 2.181, width = 0.277)]
    #[offset(forward = 0)]
    #[armament(_30X130MmR, count = 12, angle = 0)]
    M230,
    #[info(label = "Type 730 CIWS")]
    #[entity(Turret, Gun)]
    #[size(length = 5.0, width = 3.2917)]
    #[offset(forward = 0.0)]
    #[armament(_30X165MmR, forward = 1.1, count = 4, angle = 0)]
    Type730,
    #[info(label = "Turbolaser Batteries")]
    #[entity(Turret, Gun)]
    #[size(length = 1, width = 1)]
    #[offset(forward = 0)]
    #[armament(Blaster, angle = 0)]
    Turbolaser,
    #[info(label = "Sherman Turret")]
    #[entity(Turret, Gun)]
    #[size(length = 3.3, width = 2.2171875)]
    #[offset(forward = 0.4)]
    #[armament(_75X667MmR, angle = 0)]
    ShermanTurret,
    #[info(label = "Abrams Turret")]
    #[entity(Turret, Gun)]
    #[size(length = 7.93, width = 2.8)]
    #[offset(forward = 1.5)]
    #[armament(_120X570MmR, angle = 0)]
    AbramsTurret,
    #[info(label = "100mm Gun")]
    #[entity(Turret, Gun)]
    #[size(length = 6.7, width = 4.1875)]
    #[offset(forward = 1.034)]
    #[armament(_127X680MmR, forward = 2, angle = 0)]
    _100Mm,
    #[info(label = "200mm Gun")]
    #[entity(Turret, Gun)]
    #[size(length = 6.7, width = 4.1875)]
    #[offset(forward = 1.034)]
    #[armament(_200X1070MmR, forward = 2, angle = 0)]
    _200Mm,
    #[info(
        label = "2M-3M",
        link = "http://www.navweaps.com/Weapons/WNRussian_25mm-79_2m-3.php"
    )]
    #[entity(Turret, Gun)]
    #[size(length = 2.975, width = 1.72)]
    #[offset(forward = 0.5)]
    #[armament(_25X129MmR, forward = 0.5, angle = 0, external)]
    _2M3M,
    #[info(
        label = "38 cm SK C/34",
        link = "https://en.wikipedia.org/wiki/16-inch/50-caliber_Mark_7_gun"
    )]
    #[entity(Turret, Gun)]
    #[size(length = 25.6, width = 11.1)]
    #[offset(forward = 5)]
    #[armament(_380X1700MmR, forward = 12, side = 4.5, angle = 0, symmetrical, hidden)]
    _38CmSkc34,
    #[info(
        label = "45 cm/45 Type 94",
        link = "https://en.wikipedia.org/wiki/46_cm/45_Type_94_naval_gun"
    )]
    #[entity(Turret, Gun)]
    #[size(length = 30.35, width = 16.4791)]
    #[offset(forward = 4.25)]
    #[armament(
        _458X1980MmR,
        forward = 5.7,
        side = 3.1,
        angle = 0,
        symmetrical,
        hidden
    )]
    #[armament(_458X1980MmR, forward = 5.7, angle = 0, hidden)]
    _45Type94,
    #[info(
        label = "6-Pounder",
        link = "https://en.wikipedia.org/wiki/Ordnance_QF_6-pounder"
    )]
    #[entity(Turret, Gun)]
    #[size(length = 2.675, width = 1.588)]
    #[offset(forward = 0.5, side = 0.25)]
    #[armament(_57X441MmR, forward = 0.5, angle = 0, hidden)]
    _6Pounder,
    #[info(
        label = "8.8 cm SK C/35",
        link = "https://en.wikipedia.org/wiki/8.8_cm_SK_C/35_naval_gun"
    )]
    #[entity(Turret, Gun)]
    #[size(length = 3.41, width = 1.812)]
    #[offset(forward = 0.4, side = -0.1)]
    #[armament(_57X441MmR, forward = 0.5, angle = 0, hidden)]
    _88CmSkc35,
    #[info(
        label = "M1919 Browning",
        link = "https://en.wikipedia.org/wiki/M1919_Browning_machine_gun"
    )]
    #[entity(Turret, Gun)]
    #[size(length = 1.346, width = 0.715)]
    #[offset(forward = 0.212, side = -0.05)]
    #[armament(_762X54MmR, forward = 0.265, angle = 0, hidden)]
    _M1919,
    #[info(
        label = "AK-130",
        link = "https://en.wikipedia.org/wiki/AK-100_(naval_gun)"
    )]
    #[entity(Turret, Gun)]
    #[size(length = 9.45, width = 3.691)]
    #[offset(forward = 2.17111)]
    #[armament(_130X720MmR, angle = 0)]
    A190,
    #[info(label = "AK-130", link = "https://en.wikipedia.org/wiki/AK-130")]
    #[entity(Turret, Gun)]
    #[size(length = 8.45, width = 3.235)]
    #[offset(forward = 1)]
    #[armament(_130X720MmR, angle = 0)]
    Ak130,
    #[info(
        label = "50-calibre Ansaldo",
        link = "https://en.wikipedia.org/wiki/List_of_120_mm_Italian_naval_guns"
    )]
    #[entity(Turret, Gun)]
    #[size(length = 6.65, width = 3.481)]
    #[offset(forward = 1.5985)]
    #[armament(_127X680MmR, forward = 2, side = 0.3149, angle = 0, symmetrical)]
    Ansaldo,
    #[info(
        label = "BL 6-inch Mk XXIII",
        link = "https://en.wikipedia.org/wiki/BL_6-inch_Mk_XXIII_naval_gun"
    )]
    #[entity(Turret, Gun)]
    #[size(length = 11.9, width = 5.671)]
    #[offset(forward = 2)]
    #[armament(_127X680MmR, forward = 1, side = 2, angle = 0, symmetrical, external)]
    Bl6MkXxiii,
    #[info(
        label = "BL 6-inch Mk XXIII",
        link = "https://en.wikipedia.org/wiki/BL_6-inch_Mk_XXIII_naval_gun"
    )]
    #[entity(Turret, Gun)]
    #[size(length = 12.3, width = 7.207)]
    #[offset(forward = 2)]
    #[armament(_127X680MmR, forward = 1, angle = 0, external)]
    #[armament(_127X680MmR, forward = 1, side = 3, angle = 0, symmetrical, external)]
    Bl6MkXxiiiX3,
    #[info(
        label = "Bofors 57mm MK3",
        link = "https://en.wikipedia.org/wiki/Bofors_57_mm_L/70_naval_artillery_gun"
    )]
    #[entity(Turret, Gun)]
    #[size(length = 6.925, width = 4.2199)]
    #[offset(forward = 1)]
    #[armament(_57X441MmR, forward = 2, angle = 0, hidden)]
    Bofors57MmMk3,
    #[info(
        label = "Crotale",
        link = "https://en.wikipedia.org/wiki/Crotale_(missile)"
    )]
    #[entity(Turret, Missile)]
    #[size(length = 3.575, width = 2.374)]
    #[offset(forward = 0.08)]
    #[armament(Vt1, side = 0.947, angle = 0, symmetrical)]
    Crotale,
    #[info(
        label = "HQ-10",
        link = "https://en.wikipedia.org/wiki/HQ-10"
    )]
    #[entity(Turret, Sam)]
    #[size(length = 5.0, width = 3.75)]
    #[offset(forward = 0.08)]
    #[armament(Hq10SAM, count = 2, forward = 0.8)]
    Hq10,
    #[info(
        label = "H/PJ-38",
        link = "https://en.wikipedia.org/wiki/H/PJ-38_130mm_naval_gun"
    )]
    #[entity(Turret, Gun)]
    #[size(length = 12.2, width = 3.621875)]
    #[offset(forward = 3)]
    #[armament(_130X720MmR, forward = 2, angle = 0)]
    Hpj38,
    #[info(
        label = "Mark 12",
        link = "https://en.wikipedia.org/wiki/5-inch/38-caliber_gun"
    )]
    #[entity(Turret, Gun)]
    #[size(length = 7.34, width = 3.06)]
    #[offset(forward = 1)]
    #[armament(_127X680MmR, forward = 2, angle = 0)]
    Mark12,
    #[info(
        label = "Mark 12",
        link = "https://en.wikipedia.org/wiki/5-inch/38-caliber_gun"
    )]
    #[entity(Turret, Gun)]
    #[size(length = 7.6, width = 4.39375)]
    #[offset(forward = 1)]
    #[armament(_127X680MmR, forward = 2, side = 0.727, angle = 0, symmetrical)]
    Mark12X2,
    #[info(
        label = "Mark 49",
        link = "https://en.wikipedia.org/wiki/RIM-116_Rolling_Airframe_Missile"
    )]
    #[entity(Turret, Sam)]
    #[size(length = 3.02, width = 2.00547)]
    #[offset(forward = 0.15)]
    #[armament(Rim116, angle = 0, count = 8, hidden)]
    Mark49,
    #[info(
        label = "Mark 51",
        link = "https://en.wikipedia.org/wiki/Advanced_Gun_System"
    )]
    #[entity(Turret, Gun)]
    #[size(length = 11.45, width = 6.35)]
    #[offset(forward = 2.0724)]
    #[armament(Lrlap, angle = 0)]
    Mark51,
    #[info(
        label = "Mark 7",
        link = "https://en.wikipedia.org/wiki/16-inch/50-caliber_Mark_7_gun"
    )]
    #[entity(Turret, Gun)]
    #[size(length = 30.05, width = 15.26)]
    #[offset(forward = 6.5)]
    #[armament(Mark8, forward = 12, side = 3.16, angle = 0, symmetrical, hidden)]
    #[armament(Mark8, forward = 12, angle = 0, hidden)]
    Mark7,
    #[info(label = "Mark BVIII")]
    #[entity(Turret, Gun)]
    #[size(length = 18.15, width = 8.1533)]
    #[offset(forward = 4.0505)]
    #[armament(_300X1400MmR, forward = 3, side = 1.19819, angle = 0, symmetrical)]
    MarkBViii,
    #[info(
        label = "Ogon",
        link = "http://roe.ru/eng/catalog/naval-systems/shipborne-weapons/ogon/"
    )]
    #[entity(Turret, Rocket)]
    #[size(length = 2.6, width = 2.6)]
    #[armament(Of45, angle = 0, hidden)]
    #[armament(Of45, side = 0.3, angle = 0, symmetrical, hidden)]
    #[armament(Of45, side = 0.6, angle = 0, symmetrical, hidden)]
    #[armament(Of45, side = 0.9, angle = 0, symmetrical, hidden)]
    #[armament(Of45, side = 1.2, angle = 0, symmetrical, hidden)]
    Ogon,
    #[info(
        label = "OTO Melara 76 mm",
        link = "https://en.wikipedia.org/wiki/OTO_Melara_76_mm"
    )]
    #[entity(Turret, Gun)]
    #[size(length = 7.3, width = 3.0796876)]
    #[offset(forward = 1)]
    #[armament(_76X636MmR, forward = 2, angle = 0)]
    OtoMelara76Mm,
    #[info(
        label = "Komar",
        link = "https://en.wikipedia.org/wiki/9K38_Igla#Variants"
    )]
    #[entity(Turret, Missile)]
    #[size(length = 1.874, width = 2.05)]
    #[armament(Igla, side = 0.75, angle = 0, symmetrical)]
    RatepKomar,
    #[info(label = "Shtorm", link = "https://en.wikipedia.org/wiki/M-11_Shtorm")]
    #[entity(Turret, Sam)]
    #[size(length = 5.8, width = 3.1265626)]
    #[offset(forward = 0.448823)]
    #[armament(V611, forward = 0.14, side = 1.30837, angle = 0, symmetrical, external)]
    Shtorm,
    #[info(label = "Vickers MkH 12. in")]
    #[entity(Turret, Gun)]
    #[size(length = 16.65, width = 8.4551)]
    #[offset(forward = 2.3553)]
    #[armament(_300X1400MmR, forward = 3, side = 0.727, angle = 0, symmetrical)]
    VickersMkH12In,
    #[info(label = "Blaster")]
    #[entity(Weapon, Laser)]
    #[size(length = 2.0, width = 0.3)]
    #[props(speed = 1184, range = 100000)]
    Blaster,
    #[info(label = "Green Blaster")]
    #[entity(Weapon, Laser)]
    #[size(length = 2.0, width = 0.3)]
    #[props(speed = 1184, range = 100000)]
    GreenBlaster,
    #[info(label = "Vindicator Blaster")]
    #[entity(Weapon, Laser)]
    #[size(length = 2.0, width = 0.3)]
    #[props(speed = 1184, range = 100000)]
    VBlaster,
    #[info(label = "Vindicator Projector")]
    #[entity(Weapon, Shell)]
    #[size(length = 25.0, width = 0.0)]
    #[props(speed = 1500, range = 100000)]
    VProjector,
    #[info(label = "Vindicator Cluster Missiles")]
    #[entity(Weapon, Missile)]
    #[size(length = 5.0, width = 6.0)]
    #[props(speed = 1000, range = 100000)]
    VMissiles,
    #[info(label = "30 x 130 mmR")]
    #[entity(Weapon, Shell)]
    #[size(length = 0.130, width = 0.03)]
    #[props(speed = 805, range = 4000)]
    _30X130MmR,
    #[info(label = "30 x 165 mmR")]
    #[entity(Weapon, Shell)]
    #[size(length = 0.165, width = 0.03)]
    #[props(speed = 1150, range = 4500)]
    _30X165MmR,
    #[info(label = "762 x 54 mmR")]
    #[entity(Weapon, Shell)]
    #[size(length = 0.762, width = 0.05372)]
    #[props(speed = 853, range = 1400)]
    _762X54MmR,
    #[info(label = "200 x 1070 mmR")]
    #[entity(Weapon, Shell)]
    #[size(length = 1.07, width = 0.2)]
    #[props(speed = 853, range = 10000)]
    _200X1070MmR,
    #[info(label = "127 x 680 mmR")]
    #[entity(Weapon, Shell)]
    #[size(length = 0.68, width = 0.127)]
    #[offset(forward = 1)]
    #[props(speed = 790, range = 16000)]
    _127X680MmR,
    #[info(label = "130 x 720 mmR")]
    #[entity(Weapon, Shell)]
    #[size(length = 0.72, width = 0.13)]
    #[offset(forward = 1)]
    #[props(speed = 850, range = 75000)]
    _130X720MmR,
    #[info(label = "75 x 667 mmR")]
    #[entity(Weapon, TankShell)]
    #[size(length = 0.667766, width = 0.075)]
    #[offset(forward = 1)]
    #[props(speed = 618.744, range = 12000)]
    _75X667MmR,
    #[info(label = "120 x 570 mmR")]
    #[entity(Weapon, TankShell)]
    #[size(length = 0.570, width = 0.120)]
    #[offset(forward = 3)]
    #[props(speed = 1600, range = 16000)]
    _120X570MmR,
    #[info(label = "25 x 129 mmR")]
    #[entity(Weapon, Shell)]
    #[size(length = 0.1295, width = 0.0254)]
    #[props(speed = 900, range = 10000)]
    _25X129MmR,
    #[info(label = "300 x 1400 mmR")]
    #[entity(Weapon, Shell)]
    #[size(length = 1.4, width = 0.3)]
    #[props(speed = 914, range = 21500)]
    _300X1400MmR,
    #[info(label = "380 x 1700 mmR")]
    #[entity(Weapon, Shell)]
    #[size(length = 1.7, width = 0.38)]
    #[props(speed = 820, range = 35600)]
    _380X1700MmR,
    #[info(label = "458 x 1980 mmR")]
    #[entity(Weapon, Shell)]
    #[size(length = 1.98, width = 0.458)]
    #[props(speed = 780, range = 25000)]
    _458X1980MmR,
    #[info(label = "57 x 441 mmR")]
    #[entity(Weapon, Shell)]
    #[size(length = 0.441, width = 0.057)]
    #[offset(forward = 0.5)]
    #[props(speed = 853, range = 1510)]
    _57X441MmR,
    #[info(label = "76 x 636 mmR")]
    #[entity(Weapon, Shell)]
    #[size(length = 0.636, width = 0.076)]
    #[offset(forward = 1)]
    #[props(speed = 915, range = 16000)]
    _76X636MmR,
    #[info(label = "82R")]
    #[entity(Weapon, Torpedo, level = 4)]
    #[size(length = 3.275, width = 0.4605)]
    #[props(speed = 23, range = 10000)]
    #[sensors(sonar)]
    _82R,
    #[info(label = "ASROC", link = "https://en.wikipedia.org/wiki/RUR-5_ASROC")]
    #[entity(Weapon, RocketTorpedo, level = 5)]
    #[size(length = 4.5, width = 0.80859)]
    #[props(speed = 200, range = 9700, damage = 0)]
    #[armament(Mark54)]
    Asroc,
    #[info(label = "Barak 8", link = "https://en.wikipedia.org/wiki/Barak_8")]
    #[entity(Weapon, Sam, level = 4)]
    #[size(length = 4.5, width = 0.703)]
    #[props(speed = 662.6, range = 50000)]
    #[sensors(radar)]
    Barak8,
    #[info(label = "PL-12", link = "https://en.wikipedia.org/wiki/PL-12")]
    #[entity(Weapon, Sam, level = 11)]
    #[size(length = 2.5, width = 0.5)]
    #[props(speed = 1372, range = 50000)]
    #[sensors(radar)]
    Pl12,
    #[info(label = "BrahMos", link = "https://en.wikipedia.org/wiki/BrahMos")]
    #[entity(Weapon, Missile, level = 5)]
    #[size(length = 8.4, width = 0.9515625)]
    #[props(speed = 993.9, range = 650000)]
    #[sensors(radar)]
    BrahMos,
    #[info(label = "AGM-114 Hellfire", link = "https://en.wikipedia.org/wiki/AGM-114_Hellfire")]
    #[entity(Weapon, Missile, level = 7)]
    #[size(length = 1.6, width = 0.18)]
    #[props(speed = 445.9, range = 11000)]
    #[sensors(visual)]
    Hellfire,
    #[info(label = "Cannon Ball")]
    #[entity(Weapon, Shell)]
    #[size(length = 0.091, width = 0.091)]
    #[props(speed = 438.912, range = 1000)]
    CannonBall,
    #[info(label = "Depositor")]
    #[entity(Weapon, Depositor)]
    #[size(length = 21.9, width = 5.1328)]
    #[props(range = 60, reload = 0.5)]
    Depositor,
    #[info(label = "Shovel")]
    #[entity(Weapon, Shovel)]
    #[size(length = 21.9, width = 5.1328)]
    #[props(range = 60, reload = 0.5)]
    Shovel,
    #[info(label = "ESSM", link = "https://en.wikipedia.org/wiki/RIM-162_ESSM")]
    #[entity(Weapon, Sam, level = 4)]
    #[size(length = 3.66, width = 0.4575)]
    #[props(speed = 1325.2, range = 50000)]
    #[sensors(radar)]
    Essm,
    #[info(label = "Exocet", link = "https://en.wikipedia.org/wiki/Exocet")]
    #[entity(Weapon, Missile, level = 5)]
    #[size(length = 6, width = 0.9375)]
    #[props(speed = 319, range = 100000)]
    #[sensors(radar)]
    Exocet,
    #[info(
        label = "Harpoon",
        link = "https://en.wikipedia.org/wiki/Harpoon_(missile)"
    )]
    #[entity(Weapon, Missile, level = 4)]
    #[size(length = 3.8, width = 0.59375)]
    #[props(speed = 240, range = 280000)]
    #[sensors(radar)]
    Harpoon,
    #[info(label = "HQ-9", link = "https://en.wikipedia.org/wiki/HQ-9")]
    #[entity(Weapon, Sam, level = 4)]
    #[size(length = 6.8, width = 0.9562)]
    #[props(speed = 950, range = 250000)]
    #[sensors(radar)]
    Hq9,
    #[info(label = "Igla", link = "https://en.wikipedia.org/wiki/9K38_Igla")]
    #[entity(Weapon, Sam, level = 4)]
    #[size(length = 1.574, width = 0.1599)]
    #[props(speed = 570, range = 5200)]
    #[sensors(radar)]
    Igla,
    #[info(label = "Kalibr", link = "https://en.wikipedia.org/wiki/3M-54_Kalibr")]
    #[entity(Weapon, Missile, level = 4)]
    #[size(length = 8.1, width = 4.11328)]
    #[props(speed = 265.04, range = 540000)]
    #[sensors(radar)]
    Kalibr,
    #[info(label = "LRLAP")]
    #[entity(Weapon, Shell)]
    #[size(length = 2.3, width = 0.2875)]
    #[props(speed = 825, range = 140000, reload = 6)]
    Lrlap,
    #[info(label = "Magic", link = "https://en.wikipedia.org/wiki/R.550_Magic")]
    #[entity(Weapon, Sam, level = 5)]
    #[size(length = 2.72, width = 0.5)]
    #[props(speed = 1190, range = 11000)]
    #[sensors(radar)]
    Magic,
    #[info(
        label = "Mark 18",
        link = "https://en.wikipedia.org/wiki/Mark_18_torpedo"
    )]
    #[entity(Weapon, Torpedo, level = 1)]
    #[size(length = 6.2, width = 0.533)]
    #[props(speed = 14.9189, range = 18000)]
    Mark18,
    #[info(
        label = "Mark 48",
        link = "https://en.wikipedia.org/wiki/Mark_48_torpedo"
    )]
    #[entity(Weapon, Torpedo, level = 4)]
    #[size(length = 5.8, width = 0.533)]
    #[props(speed = 28.2944, range = 38000, damage = 1.33)]
    #[sensors(sonar)]
    Mark48,
    #[info(
        label = "Mark 54",
        link = "https://en.wikipedia.org/wiki/Mark_54_Lightweight_Torpedo"
    )]
    #[entity(Weapon, Torpedo, level = 4)]
    #[size(length = 2.72, width = 0.324)]
    #[props(speed = 22.63557, range = 9100)]
    #[sensors(sonar)]
    Mark54,
    #[info(
        label = "Yu-7",
        link = "https://en.wikipedia.org/wiki/Yu-7_torpedo"
    )]
    #[entity(Weapon, Torpedo, level = 7)]
    #[size(length = 2.72, width = 0.324)]
    #[props(speed = 20.0, range = 7500)]
    #[sensors(sonar)]
    Yu7,
    #[info(
        label = "Mark 8",
        link = "http://www.navweaps.com/Weapons/WNUS_16-45_mk5.php"
    )]
    #[entity(Weapon, Shell)]
    #[size(length = 1.626, width = 0.406)]
    #[props(speed = 760, range = 38000)]
    Mark8,
    #[info(
        label = "Mark 9",
        link = "https://maritime.org/doc/depthcharge9/index.htm"
    )]
    #[entity(Weapon, DepthCharge, level = 1)]
    #[size(length = 0.448056, width = 0.701675)]
    #[props(lifespan = 5)]
    Mark9,
    #[info(
        label = "Mistral",
        link = "https://en.wikipedia.org/wiki/Mistral_(missile)"
    )]
    #[entity(Weapon, Sam, level = 6)]
    #[size(length = 1.86, width = 0.1816)]
    #[props(speed = 930, range = 6000)]
    #[sensors(radar)]
    Mistral,
    #[info(
        label = "Naval Strike Missile",
        link = "https://en.wikipedia.org/wiki/Naval_Strike_Missile"
    )]
    #[entity(Weapon, Missile, level = 5)]
    #[size(length = 3.95, width = 1.049)]
    #[props(speed = 300, range = 185000, reload = 6)]
    #[sensors(radar)]
    Nsm,
    #[info(
        label = "OF-45",
        link = "http://roe.ru/eng/catalog/naval-systems/shipborne-weapons/ogon/"
    )]
    #[entity(Weapon, Rocket, level = 2)]
    #[size(length = 1.125, width = 0.29883)]
    #[props(speed = 200, range = 9810)]
    Of45,
    #[info(
        label = "RP-3",
        link = "https://en.wikipedia.org/wiki/RP-3"
    )]
    #[entity(Weapon, Rocket, level = 7)]
    #[size(length = 1.4, width = 0.08)]
    #[props(speed = 380, range = 30000)]
    RP3,
    #[info(
        label = "P-15 Termit",
        link = "https://en.wikipedia.org/wiki/P-15_Termit"
    )]
    #[entity(Weapon, Missile, level = 3)]
    #[size(length = 5.8, width = 2.084375)]
    #[props(speed = 325.85, range = 40000)]
    #[sensors(radar)]
    P15,
    #[info(
        label = "P-700 Granit",
        link = "https://en.wikipedia.org/wiki/P-700_Granit"
    )]
    #[entity(Weapon, Missile, level = 4)]
    #[size(length = 10, width = 2.96875)]
    #[props(speed = 530.08, range = 625000)]
    #[sensors(radar)]
    P700,
    #[info(label = "RBS-15", link = "https://en.wikipedia.org/wiki/RBS-15")]
    #[entity(Weapon, Missile, level = 3)]
    #[size(length = 4.33, width = 1.2516)]
    #[props(speed = 300, range = 70000)]
    #[sensors(radar)]
    Rbs15,
    #[info(
        label = "Rolling Airframe Missile",
        link = "https://en.wikipedia.org/wiki/RIM-116_Rolling_Airframe_Missile"
    )]
    #[entity(Weapon, Sam, level = 4)]
    #[size(length = 2.79, width = 0.3052)]
    #[props(speed = 680, range = 10000)]
    #[sensors(radar)]
    Rim116,
    #[info(
        label = "Vodopad",
        link = "https://en.wikipedia.org/wiki/RPK-6_Vodopad/RPK-7_Veter"
    )]
    #[entity(Weapon, RocketTorpedo, level = 6)]
    #[size(length = 6.5, width = 0.533)]
    #[props(speed = 200, range = 20000, damage = 0)]
    #[armament(_82R)]
    Rpk6,
    #[info(
        label = "S-300",
        link = "https://en.wikipedia.org/wiki/S-300_missile_system"
    )]
    #[entity(Weapon, Sam, level = 5)]
    #[size(length = 6.6, width = 1.03125)]
    #[props(speed = 950, range = 250000)]
    #[sensors(radar)]
    S300,
    #[info(
        label = "Set 65",
        link = "https://commons.wikimedia.org/wiki/File:SET-65.svg"
    )]
    #[entity(Weapon, Torpedo, level = 3)]
    #[size(length = 7.9, width = 0.533)]
    #[props(speed = 20.577778, range = 16000)]
    #[sensors(sonar)]
    Set65,
    #[info(
        label = "Tomahawk",
        link = "https://en.wikipedia.org/wiki/Tomahawk_(missile)"
    )]
    #[entity(Weapon, Missile, level = 5)]
    #[size(length = 5.56, width = 2.60625)]
    #[props(speed = 245.872, range = 250000)]
    #[sensors(radar)]
    Tomahawk,
    #[info(label = "Torped 45", link = "https://en.wikipedia.org/wiki/Torped_45")]
    #[entity(Weapon, Torpedo, level = 4)]
    #[size(length = 2.85, width = 0.4)]
    #[props(speed = 20.57779, range = 20000)]
    #[sensors(sonar)]
    Torped45,
    #[info(
        label = "Type 53",
        link = "https://en.wikipedia.org/wiki/Type_53_torpedo"
    )]
    #[entity(Weapon, Torpedo, level = 1)]
    #[size(length = 7.2, width = 0.533)]
    #[props(speed = 23.2, range = 18000)]
    Type53,
    #[info(label = "Shtorm", link = "https://en.wikipedia.org/wiki/M-11_Shtorm")]
    #[entity(Weapon, Sam, level = 4)]
    #[size(length = 6.15, width = 1.3453125)]
    #[props(speed = 600, range = 30000)]
    #[sensors(radar)]
    V611,
    #[info(
        label = "Crotale VT-1",
        link = "https://en.wikipedia.org/wiki/Crotale_(missile)"
    )]
    #[entity(Weapon, Sam, level = 5)]
    #[size(length = 2.35, width = 0.34)]
    #[props(speed = 1200, range = 6000)]
    #[sensors(radar)]
    Vt1,
    #[info(
        label = "HQ-10",
        link = "https://en.wikipedia.org/wiki/HQ-10"
    )]
    #[entity(Weapon, Sam, level = 5)]
    #[size(length = 2.0, width = 0.12)]
    #[props(speed = 686, range = 9000)]
    #[sensors(radar)]
    Hq10SAM,
    #[info(
        label = "LS-6",
        link = "https://en.wikipedia.org/wiki/LS_PGB"
    )]
    #[entity(Weapon, GlideBomb, level = 10)]
    #[size(length = 2.14, width = 1.28)]
    #[props(speed = 300, range = 2500)]
    #[sensors(radar)]
    Ls6,
    #[info(
        label = "wz. 08/39",
        link = "https://pl.wikipedia.org/wiki/Plik:Mina_morska_typu_M_1908-39.jpg"
    )]
    #[entity(Weapon, Mine, level = 3)]
    #[size(length = 2.0, width = 2.6)]
    #[props(lifespan = 300)]
    Wz0839,
    #[info(
        label = "Type 96 Bomb",
        link = "https://en.wikipedia.org/wiki/Mitsubishi_A5M"
    )]
    #[entity(Weapon, Mine, level = 3)]
    #[size(length = 1.0, width = 1.5)]
    #[props(lifespan = 15)]
    Type96Bomb,
    #[info(
        label = "Mark 82 bomb",
        link = "https://en.wikipedia.org/wiki/Mark_82_bomb"
    )]
    #[entity(Weapon, Mine, level = 10)]
    #[size(length = 2.22, width = 0.273)]
    #[props(lifespan = 20)]
    Mk82,
    #[info(label = "YJ-18", link = "https://en.wikipedia.org/wiki/YJ-18")]
    #[entity(Weapon, Missile, level = 5)]
    #[size(length = 8.1, width = 4.11328)]
    #[props(speed = 265.04, range = 540000)]
    #[sensors(radar)]
    Yj18,
}
