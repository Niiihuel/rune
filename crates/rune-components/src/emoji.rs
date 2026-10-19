//! Unicode emoji: a table with their short names and categories, the search
//! over it, the full picker of the composer and of reactions, and the small
//! grid the status editor uses.

use limn::{Component, Element, FontFamily, Icon, ScrollArea, ScrollbarAppearance, SearchField};
use rune_i18n::t;

use crate::{line, Look};

mod expressions;
mod gifs;
pub use expressions::{CatalogItem, ExpressionView, PickerTab};
pub use gifs::{GifSearchItem, GifSearchView};

/// The emoji offered by the small picker, most used first.
pub const EMOJI: [&str; 40] = [
    "👍", "❤️", "😂", "🎉", "🚀", "👀", "🦀", "🙌", "😄", "😅", "🤔", "😮", "😢", "😡", "🔥", "✨",
    "✅", "❌", "💯", "🙏", "👏", "💡", "⚡", "🐛", "📌", "📎", "☕", "🍕", "🎵", "📷", "🌙", "☀️",
    "🌧️", "🌱", "🐱", "🐶", "🤝", "👋", "🫡", "🧠",
];

/// What the account reacts with until it has used anything: the bar's
/// quick reactions and the start of the frequent ones.
pub const STARTERS: [&str; 8] = ["👍", "❤️", "😂", "🎉", "👀", "🔥", "🙏", "✅"];

/// The groups of the picker, in its order.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Category {
    People,
    Nature,
    Food,
    Activities,
    Travel,
    Objects,
    Symbols,
    Flags,
}

impl Category {
    pub const ALL: [Self; 8] = [
        Self::People,
        Self::Nature,
        Self::Food,
        Self::Activities,
        Self::Travel,
        Self::Objects,
        Self::Symbols,
        Self::Flags,
    ];

    /// The heading of its section.
    pub fn title(self) -> &'static str {
        match self {
            Self::People => t("Personas"),
            Self::Nature => t("Naturaleza"),
            Self::Food => t("Comida"),
            Self::Activities => t("Actividades"),
            Self::Travel => t("Viajes"),
            Self::Objects => t("Objetos"),
            Self::Symbols => t("Símbolos"),
            Self::Flags => t("Banderas"),
        }
    }

    /// The icon that stands for it on the rail.
    fn icon(self) -> Icon {
        match self {
            Self::People => Icon::Emoji,
            Self::Nature => Icon::Tree,
            Self::Food => Icon::FriedEgg,
            Self::Activities => Icon::Gamepad,
            Self::Travel => Icon::Car,
            Self::Objects => Icon::Lightbulb,
            Self::Symbols => Icon::Heart,
            Self::Flags => Icon::Flag,
        }
    }
}

/// One emoji of the table.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Emoji {
    pub glyph: &'static str,
    /// Its short names, the usual one first, separated by spaces.
    names: &'static str,
    pub category: Category,
}

impl Emoji {
    /// Its usual short name: "heart".
    pub fn name(&self) -> &'static str {
        self.names.split(' ').next().unwrap_or(self.names)
    }

    /// Every short name it answers to.
    pub fn names(&self) -> impl Iterator<Item = &'static str> {
        self.names.split(' ')
    }

    /// Its names as written in a message: ":heart: :red_heart:".
    pub fn codes(&self) -> String {
        self.names()
            .map(|name| format!(":{name}:"))
            .collect::<Vec<_>>()
            .join(" ")
    }
}

use Category::{
    Activities as A, Flags as F, Food as D, Nature as N, Objects as O, People as P, Symbols as S,
    Travel as T,
};

const fn e(glyph: &'static str, names: &'static str, category: Category) -> Emoji {
    Emoji {
        glyph,
        names,
        category,
    }
}

/// Every emoji the pickers and the `:` completion know, by category.
pub const TABLE: &[Emoji] = &[
    e("😀", "grinning", P),
    e("😃", "smiley", P),
    e("😄", "smile", P),
    e("😁", "grin", P),
    e("😆", "laughing satisfied", P),
    e("😅", "sweat_smile", P),
    e("🤣", "rofl rolling_on_the_floor_laughing", P),
    e("😂", "joy", P),
    e("🙂", "slight_smile", P),
    e("🙃", "upside_down", P),
    e("🫠", "melting_face", P),
    e("😉", "wink", P),
    e("😊", "blush", P),
    e("😇", "innocent", P),
    e("🥰", "smiling_face_with_hearts", P),
    e("😍", "heart_eyes", P),
    e("🤩", "star_struck", P),
    e("😘", "kissing_heart", P),
    e("😗", "kissing", P),
    e("😚", "kissing_closed_eyes", P),
    e("😋", "yum", P),
    e("😛", "stuck_out_tongue", P),
    e("😜", "stuck_out_tongue_winking_eye", P),
    e("🤪", "zany_face", P),
    e("😝", "stuck_out_tongue_closed_eyes", P),
    e("🤑", "money_mouth", P),
    e("🤗", "hugging", P),
    e("🤭", "hand_over_mouth", P),
    e("🫡", "saluting_face", P),
    e("🤫", "shushing_face", P),
    e("🤔", "thinking", P),
    e("🤐", "zipper_mouth", P),
    e("🤨", "raised_eyebrow", P),
    e("😐", "neutral_face", P),
    e("😑", "expressionless", P),
    e("😶", "no_mouth", P),
    e("😏", "smirk", P),
    e("😒", "unamused", P),
    e("🙄", "rolling_eyes", P),
    e("😬", "grimacing", P),
    e("😌", "relieved", P),
    e("😔", "pensive", P),
    e("😪", "sleepy", P),
    e("🤤", "drooling_face", P),
    e("😴", "sleeping", P),
    e("😷", "mask", P),
    e("🤒", "thermometer_face", P),
    e("🤕", "head_bandage", P),
    e("🤢", "nauseated_face", P),
    e("🤮", "vomiting_face", P),
    e("🤧", "sneezing_face", P),
    e("🥵", "hot_face", P),
    e("🥶", "cold_face", P),
    e("🥴", "woozy_face", P),
    e("😵", "dizzy_face", P),
    e("🤯", "exploding_head", P),
    e("🤠", "cowboy", P),
    e("🥳", "partying_face", P),
    e("😎", "sunglasses", P),
    e("🤓", "nerd", P),
    e("🧐", "monocle_face", P),
    e("😕", "confused", P),
    e("😟", "worried", P),
    e("🙁", "slight_frown", P),
    e("😮", "open_mouth", P),
    e("😯", "hushed", P),
    e("😲", "astonished", P),
    e("😳", "flushed", P),
    e("🥺", "pleading_face", P),
    e("😦", "frowning", P),
    e("😧", "anguished", P),
    e("😨", "fearful", P),
    e("😰", "cold_sweat", P),
    e("😥", "disappointed_relieved", P),
    e("😢", "cry", P),
    e("😭", "sob", P),
    e("😱", "scream", P),
    e("😖", "confounded", P),
    e("😣", "persevere", P),
    e("😞", "disappointed", P),
    e("😓", "sweat", P),
    e("😩", "weary", P),
    e("😫", "tired_face", P),
    e("🥱", "yawning_face", P),
    e("😤", "triumph", P),
    e("😡", "rage", P),
    e("😠", "angry", P),
    e("🤬", "cursing_face", P),
    e("😈", "smiling_imp", P),
    e("👿", "imp", P),
    e("💀", "skull", P),
    e("💩", "poop", P),
    e("🤡", "clown", P),
    e("👻", "ghost", P),
    e("👽", "alien", P),
    e("🤖", "robot", P),
    e("😺", "smiley_cat", P),
    e("😸", "smile_cat", P),
    e("😹", "joy_cat", P),
    e("😻", "heart_eyes_cat", P),
    e("🙈", "see_no_evil", P),
    e("🙉", "hear_no_evil", P),
    e("🙊", "speak_no_evil", P),
    e("👋", "wave", P),
    e("🤚", "raised_back_of_hand", P),
    e("✋", "raised_hand", P),
    e("🖖", "vulcan", P),
    e("👌", "ok_hand", P),
    e("🤌", "pinched_fingers", P),
    e("✌️", "v victory_hand", P),
    e("🤞", "crossed_fingers", P),
    e("🤟", "love_you_gesture", P),
    e("🤘", "metal", P),
    e("🤙", "call_me", P),
    e("👈", "point_left", P),
    e("👉", "point_right", P),
    e("👆", "point_up_2", P),
    e("👇", "point_down", P),
    e("☝️", "point_up", P),
    e("👍", "thumbsup +1", P),
    e("👎", "thumbsdown -1", P),
    e("✊", "fist", P),
    e("👊", "punch", P),
    e("👏", "clap", P),
    e("🙌", "raised_hands", P),
    e("🫶", "heart_hands", P),
    e("👐", "open_hands", P),
    e("🤲", "palms_up_together", P),
    e("🤝", "handshake", P),
    e("🙏", "pray", P),
    e("✍️", "writing_hand", P),
    e("💪", "muscle", P),
    e("🧠", "brain", P),
    e("👀", "eyes", P),
    e("👁️", "eye", P),
    e("👅", "tongue", P),
    e("👄", "lips", P),
    e("🗣️", "speaking_head", P),
    e("👶", "baby", P),
    e("🧑", "adult", P),
    e("👩", "woman", P),
    e("👨", "man", P),
    e("🧓", "older_adult", P),
    e("🧑‍💻", "technologist", P),
    e("🥷", "ninja", P),
    e("🧙", "mage", P),
    e("🦸", "superhero", P),
    e("🐶", "dog", N),
    e("🐱", "cat", N),
    e("🐭", "mouse", N),
    e("🐹", "hamster", N),
    e("🐰", "rabbit", N),
    e("🦊", "fox", N),
    e("🐻", "bear", N),
    e("🐼", "panda_face", N),
    e("🐨", "koala", N),
    e("🐯", "tiger", N),
    e("🦁", "lion", N),
    e("🐮", "cow", N),
    e("🐷", "pig", N),
    e("🐸", "frog", N),
    e("🐵", "monkey_face", N),
    e("🐒", "monkey", N),
    e("🐔", "chicken", N),
    e("🐧", "penguin", N),
    e("🐦", "bird", N),
    e("🐤", "baby_chick", N),
    e("🦆", "duck", N),
    e("🦉", "owl", N),
    e("🦇", "bat", N),
    e("🐺", "wolf", N),
    e("🐴", "horse", N),
    e("🦄", "unicorn", N),
    e("🐝", "bee", N),
    e("🐛", "bug", N),
    e("🦋", "butterfly", N),
    e("🐌", "snail", N),
    e("🐞", "lady_beetle", N),
    e("🐢", "turtle", N),
    e("🐍", "snake", N),
    e("🦎", "lizard", N),
    e("🐙", "octopus", N),
    e("🦑", "squid", N),
    e("🦀", "crab", N),
    e("🐠", "tropical_fish", N),
    e("🐟", "fish", N),
    e("🐬", "dolphin", N),
    e("🐳", "whale", N),
    e("🦈", "shark", N),
    e("🐊", "crocodile", N),
    e("🐘", "elephant", N),
    e("🦒", "giraffe", N),
    e("🐪", "camel", N),
    e("🦔", "hedgehog", N),
    e("🐾", "paw_prints", N),
    e("🌵", "cactus", N),
    e("🎄", "christmas_tree", N),
    e("🌲", "evergreen_tree", N),
    e("🌳", "deciduous_tree", N),
    e("🌴", "palm_tree", N),
    e("🌱", "seedling", N),
    e("🌿", "herb", N),
    e("☘️", "shamrock", N),
    e("🍀", "four_leaf_clover", N),
    e("🍁", "maple_leaf", N),
    e("🍂", "fallen_leaf", N),
    e("🍃", "leaves", N),
    e("🍄", "mushroom", N),
    e("🌷", "tulip", N),
    e("🌹", "rose", N),
    e("🥀", "wilted_flower", N),
    e("🌺", "hibiscus", N),
    e("🌸", "cherry_blossom", N),
    e("🌼", "blossom", N),
    e("🌻", "sunflower", N),
    e("🌞", "sun_with_face", N),
    e("🌝", "full_moon_with_face", N),
    e("🌚", "new_moon_with_face", N),
    e("🌙", "crescent_moon", N),
    e("⭐", "star", N),
    e("🌟", "star2 glowing_star", N),
    e("✨", "sparkles", N),
    e("⚡", "zap high_voltage", N),
    e("🔥", "fire", N),
    e("🌈", "rainbow", N),
    e("☀️", "sunny", N),
    e("⛅", "partly_sunny", N),
    e("☁️", "cloud", N),
    e("🌧️", "cloud_with_rain", N),
    e("⛈️", "thunder_cloud_and_rain", N),
    e("❄️", "snowflake", N),
    e("☃️", "snowman2", N),
    e("⛄", "snowman", N),
    e("🌊", "ocean", N),
    e("💧", "droplet", N),
    e("🌍", "earth_africa", N),
    e("🌎", "earth_americas", N),
    e("🌏", "earth_asia", N),
    e("🍏", "green_apple", D),
    e("🍎", "apple", D),
    e("🍐", "pear", D),
    e("🍊", "tangerine", D),
    e("🍋", "lemon", D),
    e("🍌", "banana", D),
    e("🍉", "watermelon", D),
    e("🍇", "grapes", D),
    e("🍓", "strawberry", D),
    e("🫐", "blueberries", D),
    e("🍒", "cherries", D),
    e("🍑", "peach", D),
    e("🥭", "mango", D),
    e("🍍", "pineapple", D),
    e("🥥", "coconut", D),
    e("🥝", "kiwi", D),
    e("🍅", "tomato", D),
    e("🥑", "avocado", D),
    e("🍆", "eggplant", D),
    e("🥔", "potato", D),
    e("🥕", "carrot", D),
    e("🌽", "corn", D),
    e("🌶️", "hot_pepper", D),
    e("🥦", "broccoli", D),
    e("🧄", "garlic", D),
    e("🧅", "onion", D),
    e("🍞", "bread", D),
    e("🥐", "croissant", D),
    e("🥖", "baguette_bread", D),
    e("🧀", "cheese", D),
    e("🥚", "egg", D),
    e("🍳", "cooking", D),
    e("🥓", "bacon", D),
    e("🥩", "cut_of_meat", D),
    e("🍗", "poultry_leg", D),
    e("🍔", "hamburger", D),
    e("🍟", "fries", D),
    e("🍕", "pizza", D),
    e("🌭", "hotdog", D),
    e("🥪", "sandwich", D),
    e("🌮", "taco", D),
    e("🌯", "burrito", D),
    e("🥗", "salad", D),
    e("🍝", "spaghetti", D),
    e("🍜", "ramen", D),
    e("🍲", "stew", D),
    e("🍣", "sushi", D),
    e("🍱", "bento", D),
    e("🥟", "dumpling", D),
    e("🍤", "fried_shrimp", D),
    e("🍙", "rice_ball", D),
    e("🍚", "rice", D),
    e("🍦", "icecream", D),
    e("🍩", "doughnut", D),
    e("🍪", "cookie", D),
    e("🎂", "birthday", D),
    e("🍰", "cake", D),
    e("🧁", "cupcake", D),
    e("🍫", "chocolate_bar", D),
    e("🍬", "candy", D),
    e("🍭", "lollipop", D),
    e("🍯", "honey_pot", D),
    e("🍿", "popcorn", D),
    e("🧉", "mate", D),
    e("☕", "coffee", D),
    e("🍵", "tea", D),
    e("🧃", "beverage_box", D),
    e("🥤", "cup_with_straw", D),
    e("🍺", "beer", D),
    e("🍻", "beers", D),
    e("🥂", "champagne_glass", D),
    e("🍷", "wine_glass", D),
    e("🍸", "cocktail", D),
    e("🧊", "ice_cube", D),
    e("🥄", "spoon", D),
    e("🍴", "fork_and_knife", D),
    e("⚽", "soccer", A),
    e("🏀", "basketball", A),
    e("🏈", "football", A),
    e("⚾", "baseball", A),
    e("🎾", "tennis", A),
    e("🏐", "volleyball", A),
    e("🏉", "rugby_football", A),
    e("🎱", "8ball", A),
    e("🏓", "ping_pong", A),
    e("🏸", "badminton", A),
    e("🥅", "goal", A),
    e("🏒", "hockey", A),
    e("⛳", "golf", A),
    e("🏹", "bow_and_arrow", A),
    e("🎣", "fishing_pole_and_fish", A),
    e("🥊", "boxing_glove", A),
    e("🛹", "skateboard", A),
    e("⛸️", "ice_skate", A),
    e("🎿", "ski", A),
    e("🏂", "snowboarder", A),
    e("🏋️", "weight_lifter", A),
    e("🚴", "bicyclist", A),
    e("🏆", "trophy", A),
    e("🥇", "first_place", A),
    e("🥈", "second_place", A),
    e("🥉", "third_place", A),
    e("🏅", "medal", A),
    e("🎖️", "military_medal", A),
    e("🎪", "circus_tent", A),
    e("🎭", "performing_arts", A),
    e("🎨", "art", A),
    e("🎬", "clapper", A),
    e("🎤", "microphone", A),
    e("🎧", "headphones", A),
    e("🎼", "musical_score", A),
    e("🎹", "musical_keyboard", A),
    e("🥁", "drum", A),
    e("🎷", "saxophone", A),
    e("🎺", "trumpet", A),
    e("🎸", "guitar", A),
    e("🎻", "violin", A),
    e("🎲", "game_die", A),
    e("♟️", "chess_pawn", A),
    e("🎯", "dart", A),
    e("🎳", "bowling", A),
    e("🎮", "video_game", A),
    e("🕹️", "joystick", A),
    e("🧩", "jigsaw", A),
    e("🎉", "tada", A),
    e("🎊", "confetti_ball", A),
    e("🎈", "balloon", A),
    e("🎁", "gift", A),
    e("🎀", "ribbon", A),
    e("🎃", "jack_o_lantern", A),
    e("🎆", "fireworks", A),
    e("🎇", "sparkler", A),
    e("🧨", "firecracker", A),
    e("🚗", "red_car", T),
    e("🚕", "taxi", T),
    e("🚙", "blue_car", T),
    e("🚌", "bus", T),
    e("🏎️", "race_car", T),
    e("🚓", "police_car", T),
    e("🚑", "ambulance", T),
    e("🚒", "fire_engine", T),
    e("🚚", "truck", T),
    e("🚜", "tractor", T),
    e("🛵", "motor_scooter", T),
    e("🏍️", "motorcycle", T),
    e("🚲", "bike", T),
    e("🛴", "scooter", T),
    e("🚨", "rotating_light", T),
    e("🚦", "vertical_traffic_light", T),
    e("🚧", "construction", T),
    e("⚓", "anchor", T),
    e("⛵", "sailboat", T),
    e("🚤", "speedboat", T),
    e("🚢", "ship", T),
    e("✈️", "airplane", T),
    e("🛫", "airplane_departure", T),
    e("🚁", "helicopter", T),
    e("🚀", "rocket", T),
    e("🛸", "flying_saucer", T),
    e("🛰️", "satellite_orbital", T),
    e("🚂", "steam_locomotive", T),
    e("🚆", "train2", T),
    e("🚇", "metro", T),
    e("🗺️", "map", T),
    e("🗽", "statue_of_liberty", T),
    e("🗼", "tokyo_tower", T),
    e("🏰", "european_castle", T),
    e("🏟️", "stadium", T),
    e("🎡", "ferris_wheel", T),
    e("🎢", "roller_coaster", T),
    e("⛲", "fountain", T),
    e("🏖️", "beach", T),
    e("🏝️", "island", T),
    e("🏜️", "desert", T),
    e("🌋", "volcano", T),
    e("⛰️", "mountain", T),
    e("🏔️", "snow_capped_mountain", T),
    e("🏕️", "camping", T),
    e("🏠", "house", T),
    e("🏡", "house_with_garden", T),
    e("🏢", "office", T),
    e("🏥", "hospital", T),
    e("🏫", "school", T),
    e("⛪", "church", T),
    e("🌃", "night_with_stars", T),
    e("🌆", "city_sunset", T),
    e("🌉", "bridge_at_night", T),
    e("🌅", "sunrise", T),
    e("🗿", "moyai", T),
    e("⌚", "watch", O),
    e("📱", "iphone mobile_phone", O),
    e("💻", "computer", O),
    e("⌨️", "keyboard", O),
    e("🖥️", "desktop", O),
    e("🖨️", "printer", O),
    e("🖱️", "mouse_three_button", O),
    e("💾", "floppy_disk", O),
    e("💿", "cd", O),
    e("📷", "camera", O),
    e("📹", "video_camera", O),
    e("🎥", "movie_camera", O),
    e("📺", "tv", O),
    e("📻", "radio", O),
    e("🎙️", "studio_microphone", O),
    e("⏰", "alarm_clock", O),
    e("⌛", "hourglass", O),
    e("📡", "satellite", O),
    e("🔋", "battery", O),
    e("🔌", "electric_plug", O),
    e("💡", "bulb", O),
    e("🔦", "flashlight", O),
    e("🕯️", "candle", O),
    e("🧯", "fire_extinguisher", O),
    e("💸", "money_with_wings", O),
    e("💵", "dollar", O),
    e("💰", "moneybag", O),
    e("💳", "credit_card", O),
    e("💎", "gem", O),
    e("🧰", "toolbox", O),
    e("🔧", "wrench", O),
    e("🔨", "hammer", O),
    e("🛠️", "tools", O),
    e("⚙️", "gear", O),
    e("🔩", "nut_and_bolt", O),
    e("🧲", "magnet", O),
    e("🧪", "test_tube", O),
    e("🔬", "microscope", O),
    e("🔭", "telescope", O),
    e("💊", "pill", O),
    e("🩹", "adhesive_bandage", O),
    e("🧹", "broom", O),
    e("🔑", "key", O),
    e("🗝️", "old_key", O),
    e("🚪", "door", O),
    e("🛏️", "bed", O),
    e("🧸", "teddy_bear", O),
    e("🛒", "shopping_cart", O),
    e("✉️", "envelope", O),
    e("📦", "package", O),
    e("📫", "mailbox", O),
    e("📝", "memo", O),
    e("📄", "page_facing_up", O),
    e("📅", "date", O),
    e("📆", "calendar", O),
    e("📊", "bar_chart", O),
    e("📈", "chart_with_upwards_trend", O),
    e("📉", "chart_with_downwards_trend", O),
    e("📋", "clipboard", O),
    e("📌", "pushpin", O),
    e("📍", "round_pushpin", O),
    e("📎", "paperclip", O),
    e("📏", "straight_ruler", O),
    e("✂️", "scissors", O),
    e("🗑️", "wastebasket", O),
    e("🔒", "lock", O),
    e("🔓", "unlock", O),
    e("🔗", "link", O),
    e("📚", "books", O),
    e("📖", "book", O),
    e("🔖", "bookmark", O),
    e("🏷️", "label", O),
    e("✏️", "pencil2", O),
    e("🖊️", "pen", O),
    e("🖌️", "paintbrush", O),
    e("🔍", "mag", O),
    e("🔔", "bell", O),
    e("📣", "mega", O),
    e("📢", "loudspeaker", O),
    e("❤️", "heart red_heart", S),
    e("🧡", "orange_heart", S),
    e("💛", "yellow_heart", S),
    e("💚", "green_heart", S),
    e("💙", "blue_heart", S),
    e("💜", "purple_heart", S),
    e("🖤", "black_heart", S),
    e("🤍", "white_heart", S),
    e("🤎", "brown_heart", S),
    e("💔", "broken_heart", S),
    e("❣️", "heart_exclamation", S),
    e("💕", "two_hearts", S),
    e("💞", "revolving_hearts", S),
    e("💓", "heartbeat", S),
    e("💗", "heartpulse", S),
    e("💖", "sparkling_heart", S),
    e("💘", "cupid", S),
    e("💝", "gift_heart", S),
    e("💯", "100", S),
    e("💢", "anger", S),
    e("💥", "boom", S),
    e("💫", "dizzy", S),
    e("💦", "sweat_drops", S),
    e("💨", "dash", S),
    e("💬", "speech_balloon", S),
    e("💭", "thought_balloon", S),
    e("💤", "zzz", S),
    e("✅", "white_check_mark", S),
    e("☑️", "ballot_box_with_check", S),
    e("✔️", "heavy_check_mark", S),
    e("❌", "x", S),
    e("❎", "negative_squared_cross_mark", S),
    e("➕", "heavy_plus_sign", S),
    e("➖", "heavy_minus_sign", S),
    e("➗", "heavy_division_sign", S),
    e("❓", "question", S),
    e("❔", "grey_question", S),
    e("❗", "exclamation", S),
    e("❕", "grey_exclamation", S),
    e("‼️", "bangbang", S),
    e("⁉️", "interrobang", S),
    e("⚠️", "warning", S),
    e("🚫", "no_entry_sign", S),
    e("⛔", "no_entry", S),
    e("♻️", "recycle", S),
    e("🔰", "beginner", S),
    e("🆗", "ok", S),
    e("🆕", "new", S),
    e("🆒", "cool", S),
    e("🆓", "free", S),
    e("🆘", "sos", S),
    e("🔴", "red_circle", S),
    e("🟠", "orange_circle", S),
    e("🟡", "yellow_circle", S),
    e("🟢", "green_circle", S),
    e("🔵", "blue_circle", S),
    e("🟣", "purple_circle", S),
    e("⚫", "black_circle", S),
    e("⚪", "white_circle", S),
    e("🟥", "red_square", S),
    e("🟩", "green_square", S),
    e("🟦", "blue_square", S),
    e("▶️", "arrow_forward", S),
    e("⏸️", "pause_button", S),
    e("⏹️", "stop_button", S),
    e("⏩", "fast_forward", S),
    e("🔁", "repeat", S),
    e("🔀", "twisted_rightwards_arrows", S),
    e("⬆️", "arrow_up", S),
    e("⬇️", "arrow_down", S),
    e("⬅️", "arrow_left", S),
    e("➡️", "arrow_right", S),
    e("↩️", "leftwards_arrow_with_hook", S),
    e("🔄", "arrows_counterclockwise", S),
    e("🎵", "musical_note", S),
    e("🎶", "notes", S),
    e("➰", "curly_loop", S),
    e("©️", "copyright", S),
    e("®️", "registered", S),
    e("™️", "tm", S),
    e("#️⃣", "hash", S),
    e("🔢", "1234", S),
    e("🔤", "abc", S),
    e("ℹ️", "information_source", S),
    e("☮️", "peace", S),
    e("☯️", "yin_yang", S),
    e("♾️", "infinity", S),
    e("🏁", "checkered_flag", F),
    e("🚩", "triangular_flag_on_post", F),
    e("🎌", "crossed_flags", F),
    e("🏴", "black_flag", F),
    e("🏳️", "white_flag", F),
    e("🏳️‍🌈", "rainbow_flag", F),
    e("🏳️‍⚧️", "transgender_flag", F),
    e("🏴‍☠️", "pirate_flag", F),
    e("🇦🇷", "flag_ar", F),
    e("🇧🇷", "flag_br", F),
    e("🇨🇱", "flag_cl", F),
    e("🇺🇾", "flag_uy", F),
    e("🇵🇾", "flag_py", F),
    e("🇧🇴", "flag_bo", F),
    e("🇵🇪", "flag_pe", F),
    e("🇨🇴", "flag_co", F),
    e("🇻🇪", "flag_ve", F),
    e("🇪🇨", "flag_ec", F),
    e("🇲🇽", "flag_mx", F),
    e("🇪🇸", "flag_es", F),
    e("🇺🇸", "flag_us", F),
    e("🇬🇧", "flag_gb", F),
    e("🇫🇷", "flag_fr", F),
    e("🇩🇪", "flag_de", F),
    e("🇮🇹", "flag_it", F),
    e("🇵🇹", "flag_pt", F),
    e("🇯🇵", "flag_jp", F),
    e("🇨🇦", "flag_ca", F),
    e("🇪🇺", "flag_eu", F),
    e("🇺🇳", "united_nations", F),
];

/// The entry of `glyph`, when the table has it.
pub fn find(glyph: &str) -> Option<&'static Emoji> {
    TABLE.iter().find(|emoji| emoji.glyph == glyph)
}

/// How a message writes `glyph`: ":heart:", or the glyph itself when it
/// has no name here.
pub fn code(glyph: &str) -> String {
    find(glyph).map_or_else(|| glyph.to_owned(), |emoji| format!(":{}:", emoji.name()))
}

/// The emoji whose names match `query`, best first: a name that is it, one
/// that starts with it, then a word of a name that does (`heart` in
/// `mending_heart`), then a name that holds it; each in the table's order.
pub fn search(query: &str) -> Vec<&'static Emoji> {
    let query = query.trim().trim_matches(':').to_lowercase();
    if query.is_empty() {
        return Vec::new();
    }
    let rank = |emoji: &Emoji| {
        emoji
            .names()
            .filter_map(|name| {
                if name == query {
                    Some(0)
                } else if name.starts_with(&query) {
                    Some(1)
                } else if name.split('_').any(|word| word.starts_with(&query)) {
                    Some(2)
                } else {
                    name.contains(&query).then_some(3)
                }
            })
            .min()
    };
    let mut found: Vec<(u8, usize, &'static Emoji)> = TABLE
        .iter()
        .enumerate()
        .filter_map(|(index, emoji)| rank(emoji).map(|rank| (rank, index, emoji)))
        .collect();
    found.sort_by_key(|(rank, index, _)| (*rank, *index));
    found.into_iter().map(|(_, _, emoji)| emoji).collect()
}

/// The emoji used lately, then the starters, without repeats: the frequent
/// ones of the pickers and of the message menu.
pub fn frequent(recent: &[String], count: usize) -> Vec<String> {
    let mut list: Vec<String> = Vec::with_capacity(count);
    for glyph in recent
        .iter()
        .map(String::as_str)
        .chain(STARTERS.iter().copied())
    {
        if list.len() == count {
            break;
        }
        if !list.iter().any(|kept| kept == glyph) {
            list.push(glyph.to_owned());
        }
    }
    list
}

const PER_ROW: usize = 8;
const CELL: f32 = 36.0;

/// The width of the small picker's grid.
pub const WIDTH: f32 = CELL * PER_ROW as f32 + 4.0 * (PER_ROW - 1) as f32 + 16.0;

/// The small grid of [`EMOJI`]: each emoji reports `{prefix}{emoji}` when
/// chosen. The status editor offers it.
pub fn picker(prefix: &str, title: &str, look: &Look) -> Element {
    let rows: Vec<Element> = EMOJI
        .chunks(PER_ROW)
        .map(|chunk| {
            Element::row(
                chunk
                    .iter()
                    .map(|emoji| {
                        Element::clickable(
                            format!("{prefix}{emoji}"),
                            crate::centered(
                                Element::text(*emoji)
                                    .font_family(FontFamily::Sans)
                                    .font_size(20.0)
                                    .foreground(look.text),
                            ),
                        )
                        .size(Some(CELL), Some(CELL))
                        .radius(6.0)
                        .hover_background(look.selected)
                        .accessibility_label(*emoji)
                    })
                    .collect(),
            )
            .gap(4.0)
        })
        .collect();
    let mut column = vec![line(title, 12.0, look.muted)];
    column.extend(rows);
    Element::column(column)
        .gap(4.0)
        .padding(8.0)
        .size(Some(WIDTH), None)
}

/// The ids of a full picker, all derived from its `base`.
pub mod ids {
    /// The action of choosing `glyph`.
    pub fn pick(base: &str, glyph: &str) -> String {
        format!("{base}={glyph}")
    }

    /// Its search field.
    pub fn search(base: &str) -> String {
        format!("{base}:search")
    }

    /// The button of the rail that goes to section `index`.
    pub fn category(base: &str, index: usize) -> String {
        format!("{base}:cat={index}")
    }

    /// Where section `index` starts, to scroll to.
    pub fn section(base: &str, index: usize) -> String {
        format!("{base}:section={index}")
    }

    /// Its scrolling grid.
    pub fn grid(base: &str) -> String {
        format!("{base}:grid")
    }
}

/// The full picker of the composer (530×517.5) and of reactions (535×543).
pub struct PickerView<'a> {
    /// Every action of the picker starts with it; see [`ids`].
    pub base: &'a str,
    /// What is typed in its search.
    pub query: &'a str,
    /// The emoji used lately, most recent first.
    pub recent: &'a [String],
    pub placeholder: &'a str,
    /// Show the composer's expression tabs; the reactions' picker omits them.
    pub tabs: bool,
    /// Real catalog/media content for the composer's additional tabs.
    pub expressions: Option<ExpressionView<'a>>,
}

/// The full picker's surface: the composer's, and the reactions' a little
/// larger. The `Popover` that holds it is this wide; the picker fills it
/// inside the popover's padding.
pub fn picker_size(tabs: bool) -> (f32, f32) {
    if tabs {
        (530.0, 517.5)
    } else {
        (535.0, 543.0)
    }
}

/// The padding limn's `Popover` keeps around what it holds.
const POPOVER_PADDING: f32 = 6.0;
const GRID_COLUMNS: usize = 9;
const GRID_CELL: f32 = 51.0;
const RAIL: f32 = 53.0;
const FOOTER: f32 = 50.5;

/// The full picker: tabs (composer only), its search, the rail of
/// categories beside the grid of sections, and a footer naming the
/// highlighted emoji: the first found, or the first frequent one. Search
/// replaces the sections with what matches.
pub fn full_picker(view: PickerView<'_>, look: &Look) -> Element {
    let (width, height) = picker_size(view.tabs);
    let base = view.base;
    let mut header = Vec::new();
    if let Some(expressions) = &view.expressions {
        if expressions.tab != PickerTab::Emoji {
            return expressions::media_picker(&view, look);
        }
    }
    if view.tabs {
        let tab = Element::panel(crate::centered(
            Element::text(t("Emojis"))
                .font_family(FontFamily::SansMedium)
                .font_size(14.0)
                .foreground(look.text),
        ))
        .size(Some(68.0), Some(32.0))
        .radius(6.0)
        .background(look.selected);
        header.push(if let Some(expressions) = &view.expressions {
            expressions::tabs(base, expressions.tab, look)
        } else {
            Element::row(vec![tab, Element::spacer().flex(1.0)])
                .size(None, Some(32.0))
                .padding(0.0)
        });
    }
    let field = SearchField::new(
        ids::search(base),
        view.query,
        view.placeholder,
        look.menu_palette(),
    )
    .width(289.0)
    .accessibility_label(t("Buscar emoji"))
    .render();
    header.push(Element::row(vec![field, Element::spacer().flex(1.0)]).size(None, Some(38.5)));
    let searching = !view.query.trim().is_empty();
    let frequent = frequent(view.recent, GRID_COLUMNS * 2);
    let found: Vec<&'static str> = if searching {
        search(view.query)
            .into_iter()
            .map(|emoji| emoji.glyph)
            .collect()
    } else {
        Vec::new()
    };
    let highlighted = if searching {
        found.first().copied()
    } else {
        frequent.first().map(String::as_str)
    };
    let mut sections = Vec::new();
    if let Some(expressions) = &view.expressions {
        if !expressions.custom.is_empty() {
            sections.push(expressions::catalog(
                base,
                "custom",
                t("Emojis del servidor"),
                expressions.custom,
                view.query,
                look,
            ));
        }
    }
    if searching {
        if found.is_empty() {
            sections.push(
                Element::panel(line(t("No se encontró ningún emoji."), 14.0, look.muted))
                    .padding(12.0),
            );
        } else {
            sections.push(grid(base, &found, highlighted, look));
        }
    } else {
        let glyphs: Vec<&str> = frequent.iter().map(String::as_str).collect();
        sections.push(section(
            base,
            0,
            t("Usado con frecuencia"),
            &glyphs,
            highlighted,
            look,
        ));
        for (index, category) in Category::ALL.iter().enumerate() {
            let glyphs: Vec<&str> = TABLE
                .iter()
                .filter(|emoji| emoji.category == *category)
                .map(|emoji| emoji.glyph)
                .collect();
            sections.push(section(
                base,
                index + 1,
                category.title(),
                &glyphs,
                None,
                look,
            ));
        }
    }
    // The grid takes nine cells beside the rail, edge to edge.
    let grid_area = ScrollArea::new(ids::grid(base), Element::column(sections))
        .appearance(ScrollbarAppearance::overlay().buttons(false))
        .render()
        .flex(1.0);
    let clock = Element::icon(Icon::Clock)
        .large_icon(20.0)
        .foreground(look.faint);
    let mut rail = vec![rail_button(base, 0, clock, t("Usado con frecuencia"), look)];
    for (index, category) in Category::ALL.iter().enumerate() {
        let glyph = Element::icon(category.icon())
            .large_icon(20.0)
            .foreground(look.faint);
        rail.push(rail_button(base, index + 1, glyph, category.title(), look));
    }
    // It scrolls where the body is shorter than its buttons.
    let rail = ScrollArea::new(
        format!("{base}:rail"),
        Element::column(rail).gap(4.0).padding(8.0),
    )
    .appearance(ScrollbarAppearance::overlay().buttons(false))
    .render()
    .size(Some(RAIL), None)
    .background(look.raised);
    let body = Element::row(vec![rail, grid_area]).flex(1.0);
    let footer = match highlighted.and_then(find) {
        Some(emoji) => Element::row(vec![
            Element::text(emoji.glyph)
                .font_family(FontFamily::Sans)
                .font_size(30.0)
                .foreground(look.text)
                .centered_y(),
            Element::truncated_text(emoji.codes())
                .font_family(FontFamily::SansMedium)
                .font_size(16.0)
                .foreground(look.text)
                .centered_y()
                .flex(1.0),
        ])
        .gap(10.0),
        None => Element::spacer(),
    };
    let footer = Element::panel(footer)
        .padding(10.0)
        .size(None, Some(FOOTER))
        .background(look.canvas);
    // The rail, the grid and the footer reach the sides; the header is inset.
    Element::column(vec![
        Element::column(header).gap(12.0).padding(12.0),
        body,
        footer,
    ])
    .size(
        Some(width - 2.0 * POPOVER_PADDING),
        Some(height - 2.0 * POPOVER_PADDING),
    )
    .radius(8.0)
    .background(look.menu)
}

fn rail_button(base: &str, index: usize, glyph: Element, title: &str, look: &Look) -> Element {
    let button = Element::clickable(ids::category(base, index), crate::centered(glyph))
        .size(Some(36.0), Some(36.0))
        .radius(6.0)
        .hover_background(look.hover)
        .accessible(limn::Access::new(limn::Role::Button).name(title.to_owned()));
    limn::Tooltip::new(
        button,
        ids::category(base, index),
        title,
        look.menu_palette(),
    )
    .placement(limn::TooltipPlacement::Right)
    .render()
}

fn section(
    base: &str,
    index: usize,
    title: &str,
    glyphs: &[&str],
    highlighted: Option<&str>,
    look: &Look,
) -> Element {
    let heading = Element::row(vec![
        Element::text(title.to_owned())
            .font_family(FontFamily::SansMedium)
            .font_size(14.0)
            .foreground(look.text)
            .centered_y(),
        Element::icon(Icon::ChevronDown)
            .icon_size(12.0)
            .foreground(look.muted)
            .centered_y(),
    ])
    .gap(6.0)
    .padding(4.0)
    .size(None, Some(28.0));
    Element::column(vec![heading, grid(base, glyphs, highlighted, look)])
        .anchor(ids::section(base, index))
}

fn grid(base: &str, glyphs: &[&str], highlighted: Option<&str>, look: &Look) -> Element {
    let rows = glyphs
        .chunks(GRID_COLUMNS)
        .map(|chunk| {
            let mut cells: Vec<Element> = chunk
                .iter()
                .map(|glyph| cell(base, glyph, highlighted == Some(*glyph), look))
                .collect();
            cells.push(Element::spacer().flex(1.0));
            Element::row(cells)
        })
        .collect();
    Element::column(rows)
}

fn cell(base: &str, glyph: &str, highlighted: bool, look: &Look) -> Element {
    let id = ids::pick(base, glyph);
    let mut button = Element::clickable(
        id.clone(),
        crate::centered(
            Element::text(glyph.to_owned())
                .font_family(FontFamily::Sans)
                .font_size(32.0)
                .foreground(look.text),
        ),
    )
    .size(Some(GRID_CELL), Some(GRID_CELL))
    .radius(6.0)
    .hover_background(look.selected)
    .accessibility_label(code(glyph));
    if highlighted {
        button = button.background(look.selected);
    }
    limn::Tooltip::new(button, id, code(glyph), look.menu_palette()).render()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{controls, frame};
    use limn::Size;

    #[test]
    fn every_emoji_of_the_small_picker_is_a_control_with_the_prefix() {
        let (_, frame) = frame(
            Element::row(vec![
                picker("emoji:", "Reaccionar", &Look::DARK),
                Element::spacer().flex(1.0),
            ]),
            Size::new(400.0, 400.0),
        );
        let chosen: Vec<String> = controls(&frame)
            .into_iter()
            .filter(|id| id.starts_with("emoji:"))
            .collect();
        assert_eq!(chosen.len(), EMOJI.len());
        assert_eq!(chosen[0], "emoji:👍");
    }

    #[test]
    fn a_search_puts_names_that_start_with_it_first_then_words_then_the_rest() {
        let found: Vec<&str> = search(":hea").iter().map(|emoji| emoji.name()).collect();
        assert_eq!(found[0], "heart_eyes", "{found:?}");
        assert_eq!(search("heart")[0].name(), "heart", "the very name first");
        let heart = found.iter().position(|name| *name == "heart").unwrap();
        let broken = found
            .iter()
            .position(|name| *name == "broken_heart")
            .unwrap();
        assert!(heart < broken, "{found:?}");
        assert!(search("").is_empty());
        assert_eq!(code("❤️"), ":heart:");
        assert_eq!(code("🜁"), "🜁", "a glyph the table lacks writes itself");
    }

    #[test]
    fn the_table_has_no_repeated_glyph_and_every_category() {
        for (index, emoji) in TABLE.iter().enumerate() {
            assert!(
                TABLE[..index]
                    .iter()
                    .all(|other| other.glyph != emoji.glyph),
                "{} twice",
                emoji.glyph
            );
        }
        for category in Category::ALL {
            assert!(TABLE.iter().any(|emoji| emoji.category == category));
        }
    }

    #[test]
    fn frequent_ones_are_the_recent_then_the_starters_without_repeats() {
        let recent = vec!["🦀".to_owned(), "👍".to_owned()];
        assert_eq!(frequent(&recent, 4), vec!["🦀", "👍", "❤️", "😂"]);
    }

    #[test]
    fn the_full_picker_offers_its_search_categories_and_the_highlighted_emoji() {
        let recent = vec!["🦀".to_owned()];
        let view = |query| PickerView {
            base: "react-pick",
            query,
            recent: &recent,
            placeholder: "Encontrá la reacción perfecta",
            tabs: false,
            expressions: None,
        };
        let (_, frame) = frame(
            Element::column(vec![full_picker(view(""), &Look::DARK)]),
            Size::new(700.0, 700.0),
        );
        let ids = controls(&frame);
        assert!(ids.contains(&"react-pick:search".to_owned()), "{ids:?}");
        assert!(ids.contains(&"react-pick:cat=3".to_owned()));
        assert!(ids.contains(&"react-pick=🦀".to_owned()));
        let texts = crate::testing::texts(&frame);
        assert!(
            texts.iter().any(|text| text == ":crab:"),
            "the footer names it"
        );
        let (_, found) = frame_of(full_picker(view("fire"), &Look::DARK));
        assert!(controls(&found).contains(&"react-pick=🔥".to_owned()));
        assert!(!controls(&found).contains(&"react-pick=🦀".to_owned()));
    }

    fn frame_of(element: Element) -> (limn::UiTree, limn::Frame) {
        frame(Element::column(vec![element]), Size::new(700.0, 700.0))
    }
}
