use crate::domain::{FigureKind, GameLine};
use crate::figures::formats::INFINITY_IMAGE_BYTES;
use crate::figures::skylanders::crypto::{aes128_decrypt_block, aes128_encrypt_block};

const BLOCK_BYTES: usize = 16;
const FIRST_BLOCK_ACCESS: u32 = 0x17878e;
const OTHER_BLOCK_ACCESS: u32 = 0x778788;
const SHA1_KEY_PREFIX: &[u8; 31] =
    b"\xaf\x62\xd2\xec\x04\x91\x96\x8c\xc5\x2a\x1a\x71\x65\xf8\x65\xfe\x28\x63\x29 Disney 2013";
const UID_STATIC_SUFFIX: [u8; 4] = [0x89, 0x44, 0x00, 0xc2];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FigureCatalogEntry {
    pub index: u16,
    pub game_line: GameLine,
    pub kind: FigureKind,
    pub series: &'static str,
    pub name: &'static str,
    pub figure_number: u32,
}

macro_rules! entry {
    ($index:expr, $kind:ident, $series:expr, $name:expr, $figure_number:expr) => {
        FigureCatalogEntry {
            index: $index,
            game_line: GameLine::Infinity,
            kind: FigureKind::$kind,
            series: $series,
            name: $name,
            figure_number: $figure_number,
        }
    };
}

#[rustfmt::skip]
pub const INFINITY_CATALOG: &[FigureCatalogEntry] = &[
    entry!(0, Character, "The Incredibles", "Mr. Incredible", 0x0f4241),
    entry!(1, Character, "Monsters University", "Sulley", 0x0f4242),
    entry!(2, Character, "Pirates of the Caribbean", "Jack Sparrow", 0x0f4243),
    entry!(3, Character, "The Lone Ranger", "The Lone Ranger", 0x0f4244),
    entry!(4, Character, "The Lone Ranger", "Tonto", 0x0f4245),
    entry!(5, Character, "Cars", "Lightning McQueen", 0x0f4246),
    entry!(6, Character, "Cars", "Holley Shiftwell", 0x0f4247),
    entry!(7, Character, "Toy Story", "Buzz Lightyear", 0x0f4248),
    entry!(8, Character, "Toy Story", "Jessie", 0x0f4249),
    entry!(9, Character, "Monsters University", "Mike Wazowski", 0x0f424a),
    entry!(10, Character, "The Incredibles", "Mrs. Incredible", 0x0f424b),
    entry!(11, Character, "Pirates of the Caribbean", "Barbossa", 0x0f424c),
    entry!(12, Character, "Pirates of the Caribbean", "Davy Jones", 0x0f424d),
    entry!(13, Character, "Monsters University", "Randy", 0x0f424e),
    entry!(14, Character, "The Incredibles", "Syndrome", 0x0f424f),
    entry!(15, Character, "Toy Story", "Woody", 0x0f4250),
    entry!(16, Character, "Cars", "Mater", 0x0f4251),
    entry!(17, Character, "The Incredibles", "Dash", 0x0f4252),
    entry!(18, Character, "The Incredibles", "Violet", 0x0f4253),
    entry!(19, Character, "Cars", "Francesco Bernoulli", 0x0f4254),
    entry!(20, Character, "Fantasia", "Sorcerer's Apprentice Mickey", 0x0f4255),
    entry!(21, Character, "The Nightmare Before Christmas", "Jack Skellington", 0x0f4256),
    entry!(22, Character, "Tangled", "Rapunzel", 0x0f4257),
    entry!(23, Character, "Frozen", "Anna", 0x0f4258),
    entry!(24, Character, "Frozen", "Elsa", 0x0f4259),
    entry!(25, Character, "Phineas and Ferb", "Phineas Flynn", 0x0f425a),
    entry!(26, Character, "Phineas and Ferb", "Agent P", 0x0f425b),
    entry!(27, Character, "Wreck-It Ralph", "Wreck-It Ralph", 0x0f425c),
    entry!(28, Character, "Wreck-It Ralph", "Vanellope von Schweetz", 0x0f425d),
    entry!(29, Character, "The Incredibles", "Mr. Incredible (Crystal)", 0x0f425e),
    entry!(30, Character, "Pirates of the Caribbean", "Jack Sparrow (Crystal)", 0x0f425f),
    entry!(31, Character, "Monsters University", "Sulley (Crystal)", 0x0f4260),
    entry!(32, Character, "Cars", "Lightning McQueen (Crystal)", 0x0f4261),
    entry!(33, Character, "The Lone Ranger", "The Lone Ranger (Crystal)", 0x0f4262),
    entry!(34, Character, "Toy Story", "Buzz Lightyear (Crystal)", 0x0f4263),
    entry!(35, Character, "Phineas and Ferb", "Agent P (Crystal)", 0x0f4264),
    entry!(36, Character, "Fantasia", "Sorcerer's Apprentice Mickey (Crystal)", 0x0f4265),
    entry!(37, Character, "Toy Story", "Buzz Lightyear (Glowing)", 0x0f4266),
    entry!(38, LevelPiece, "Play Set", "The Incredibles - Pirates of the Caribbean - Monsters University Play Set", 0x1e8481),
    entry!(39, LevelPiece, "Play Set", "The Lone Ranger Play Set", 0x1e8482),
    entry!(40, LevelPiece, "Play Set", "Cars Play Set", 0x1e8483),
    entry!(41, LevelPiece, "Play Set", "Toy Story in Space Play Set", 0x1e8484),
    entry!(42, PowerDisc, "Power Disc", "Bolt - Bolt's Super Strength - Ability", 0x2dc6c3),
    entry!(43, PowerDisc, "Power Disc", "Wreck-It Ralph - Ralph's Power of Destruction - Ability", 0x2dc6c4),
    entry!(44, PowerDisc, "Power Disc", "Fantasia - Chernabog's Power - Ability", 0x2dc6c5),
    entry!(45, PowerDisc, "Power Disc", "Cars - C.H.R.O.M.E. Damage Increaser - Ability", 0x2dc6c6),
    entry!(46, PowerDisc, "Power Disc", "Phineas and Ferb - Dr. Doofenshmirtz's Damage-Inator! - Ability", 0x2dc6c7),
    entry!(47, PowerDisc, "Power Disc", "Frankenweenie - Electro-Charge - Ability", 0x2dc6c8),
    entry!(48, PowerDisc, "Power Disc", "Wreck-It Ralph - Fix-It Felix's Repair Power - Ability", 0x2dc6c9),
    entry!(49, PowerDisc, "Power Disc", "Tangled - Rapunzel's Healing - Ability", 0x2dc6ca),
    entry!(50, PowerDisc, "Power Disc", "Cars - C.H.R.O.M.E. Armor Shield - Ability", 0x2dc6cb),
    entry!(51, PowerDisc, "Power Disc", "Toy Story - Star Command Shield - Ability", 0x2dc6cc),
    entry!(52, PowerDisc, "Power Disc", "The Incredibles - Violet's Force Field - Ability", 0x2dc6cd),
    entry!(53, PowerDisc, "Power Disc", "Pirates of the Caribbean - Pieces of Eight - Ability", 0x2dc6ce),
    entry!(54, PowerDisc, "Power Disc", "DuckTales - Scrooge McDuck's Lucky Dime - Ability", 0x2dc6cf),
    entry!(55, PowerDisc, "Power Disc", "TRON - User Control Disc - Ability", 0x2dc6d0),
    entry!(56, PowerDisc, "Power Disc", "Fantasia - Mickey's Sorcerer Hat - Ability", 0x2dc6d1),
    entry!(57, PowerDisc, "Power Disc", "Toy Story - Emperor Zurg's Wrath - Ability", 0x2dc6fe),
    entry!(58, PowerDisc, "Power Disc", "The Sword in the Stone - Merlin's Summon - Ability", 0x2dc6ff),
    entry!(59, PowerDisc, "Power Disc", "Mickey Mouse Universe - Mickey's Car - Toy (Vehicle)", 0x3d0912),
    entry!(60, PowerDisc, "Power Disc", "Cinderella - Cinderella's Coach - Toy (Vehicle)", 0x3d0913),
    entry!(61, PowerDisc, "Power Disc", "The Muppets - Electric Mayhem Bus - Toy (Vehicle)", 0x3d0914),
    entry!(62, PowerDisc, "Power Disc", "101 Dalmatians - Cruella De Vil's Car - Toy (Vehicle)", 0x3d0915),
    entry!(63, PowerDisc, "Power Disc", "Toy Story - Pizza Planet Delivery Truck - Toy (Vehicle)", 0x3d0916),
    entry!(64, PowerDisc, "Power Disc", "Monsters, Inc. - Mike's New Car - Toy (Vehicle)", 0x3d0917),
    entry!(65, PowerDisc, "Power Disc", "Disney Parks - Disney Parks Parking Lot Tram - Toy (Vehicle)", 0x3d0919),
    entry!(66, PowerDisc, "Power Disc", "Peter Pan, Disney Parks - Jolly Roger - Toy (Aircraft)", 0x3d091a),
    entry!(67, PowerDisc, "Power Disc", "Dumbo, Disney Parks - Dumbo the Flying Elephant - Toy (Aircraft)", 0x3d091b),
    entry!(68, PowerDisc, "Power Disc", "Bolt - Calico Helicopter - Toy (Aircraft)", 0x3d091c),
    entry!(69, PowerDisc, "Power Disc", "Tangled - Maximus - Toy (Mount)", 0x3d091d),
    entry!(70, PowerDisc, "Power Disc", "Brave - Angus - Toy (Mount)", 0x3d091e),
    entry!(71, PowerDisc, "Power Disc", "Aladdin - Abu the Elephant - Toy (Mount)", 0x3d091f),
    entry!(72, PowerDisc, "Power Disc", "The Adventures of Ichabod and Mr. Toad - Headless Horseman's Horse - Toy (Mount)", 0x3d0920),
    entry!(73, PowerDisc, "Power Disc", "Beauty and the Beast - Phillipe - Toy (Mount)", 0x3d0921),
    entry!(74, PowerDisc, "Power Disc", "Mulan - Khan - Toy (Mount)", 0x3d0922),
    entry!(75, PowerDisc, "Power Disc", "Tarzan - Tantor - Toy (Mount)", 0x3d0923),
    entry!(76, PowerDisc, "Power Disc", "Mulan - Dragon Firework Cannon - Toy (Weapon)", 0x3d0924),
    entry!(77, PowerDisc, "Power Disc", "Lilo & Stitch - Stitch's Blaster - Toy (Weapon)", 0x3d0925),
    entry!(78, PowerDisc, "Power Disc", "Toy Story, Disney Parks - Toy Story Mania Blaster - Toy (Weapon)", 0x3d0926),
    entry!(79, PowerDisc, "Power Disc", "Alice in Wonderland - Flamingo Croquet Mallet - Toy (Weapon)", 0x3d0927),
    entry!(80, PowerDisc, "Power Disc", "Up - Carl Fredricksen's Cane - Toy (Weapon)", 0x3d0928),
    entry!(81, PowerDisc, "Power Disc", "Lilo & Stitch - Hangin' Ten Stitch With Surfboard - Toy (Hoverboard)", 0x3d0929),
    entry!(82, PowerDisc, "Power Disc", "Condorman - Condorman Glider - Toy (Glider)", 0x3d092a),
    entry!(83, PowerDisc, "Power Disc", "WALL-E - WALL-E's Fire Extinguisher - Toy (Jetpack)", 0x3d092b),
    entry!(84, PowerDisc, "Power Disc", "TRON - On the Grid - Customization (Terrain)", 0x3d092c),
    entry!(85, PowerDisc, "Power Disc", "WALL-E - WALL-E's Collection - Customization (Terrain)", 0x3d092d),
    entry!(86, PowerDisc, "Power Disc", "Wreck-It Ralph - King Candy's Dessert Toppings - Customization (Terrain)", 0x3d092e),
    entry!(87, PowerDisc, "Power Disc", "Frankenweenie - Victor's Experiments - Customization (Terrain)", 0x3d0930),
    entry!(88, PowerDisc, "Power Disc", "The Nightmare Before Christmas - Jack's Scary Decorations - Customization (Terrain)", 0x3d0931),
    entry!(89, PowerDisc, "Power Disc", "Frozen - Frozen Flourish - Customization (Terrain)", 0x3d0933),
    entry!(90, PowerDisc, "Power Disc", "Tangled - Rapunzel's Kingdom - Customization (Terrain)", 0x3d0934),
    entry!(91, PowerDisc, "Power Disc", "TRON - TRON Interface - Customization (Skydome)", 0x3d0935),
    entry!(92, PowerDisc, "Power Disc", "WALL-E - Buy N Large Atmosphere - Customization (Skydome)", 0x3d0936),
    entry!(93, PowerDisc, "Power Disc", "Wreck-It Ralph - Sugar Rush Sky - Customization (Skydome)", 0x3d0937),
    entry!(94, PowerDisc, "Power Disc", "The Nightmare Before Christmas - Halloween Town Sky - Customization (Skydome)", 0x3d093a),
    entry!(95, PowerDisc, "Power Disc", "Frozen - Chill in the Air - Customization (Skydome)", 0x3d093c),
    entry!(96, PowerDisc, "Power Disc", "Tangled - Rapunzel's Birthday Sky - Customization (Skydome)", 0x3d093d),
    entry!(97, PowerDisc, "Power Disc", "Toy Story, Disney Parks - Astro Blasters Space Cruiser - Toy (Vehicle)", 0x3d0940),
    entry!(98, PowerDisc, "Power Disc", "Finding Nemo - Marlin's Reef - Customization (Terrain)", 0x3d0941),
    entry!(99, PowerDisc, "Power Disc", "Finding Nemo - Nemo's Seascape - Customization (Skydome)", 0x3d0942),
    entry!(100, PowerDisc, "Power Disc", "Alice in Wonderland - Alice's Wonderland - Customization (Terrain)", 0x3d0943),
    entry!(101, PowerDisc, "Power Disc", "Alice in Wonderland - Tulgey Wood - Customization (Skydome)", 0x3d0944),
    entry!(102, PowerDisc, "Power Disc", "Phineas and Ferb - Tri-State Area Terrain", 0x3d0945),
    entry!(103, PowerDisc, "Power Disc", "Phineas and Ferb - Danville Sky", 0x3d0946),
];

pub fn infinity_catalog_entry(index: u16) -> Option<&'static FigureCatalogEntry> {
    INFINITY_CATALOG.get(index as usize)
}

pub fn find_infinity_catalog_entry(figure_number: u32) -> Option<&'static FigureCatalogEntry> {
    INFINITY_CATALOG
        .iter()
        .find(|entry| entry.figure_number == figure_number)
}

pub fn initialize_infinity_entity_image(
    figure_number: u32,
    entity_id: u32,
) -> [u8; INFINITY_IMAGE_BYTES] {
    let mut image = [0; INFINITY_IMAGE_BYTES];
    let uid = deterministic_uid(figure_number, entity_id);
    image[..7].copy_from_slice(&uid);
    image[7..11].copy_from_slice(&UID_STATIC_SUFFIX);

    write_access_bytes(&mut image, 0, FIRST_BLOCK_ACCESS);
    for sector in 1..5 {
        write_access_bytes(&mut image, sector, OTHER_BLOCK_ACCESS);
    }

    let figure_data = blank_figure_data(figure_number);
    let key = infinity_figure_key(uid);
    let encrypted_figure_data = aes128_encrypt_block(&figure_data, &key);
    let encrypted_blank = aes128_encrypt_block(&[0; BLOCK_BYTES], &key);

    image[0x10..0x20].copy_from_slice(&encrypted_figure_data);
    for block in [0x04usize, 0x08, 0x0c, 0x0d] {
        let offset = block * BLOCK_BYTES;
        image[offset..offset + BLOCK_BYTES].copy_from_slice(&encrypted_blank);
    }

    image
}

pub fn decrypt_infinity_figure_data(image: &[u8; INFINITY_IMAGE_BYTES]) -> [u8; BLOCK_BYTES] {
    let mut uid = [0; 7];
    uid.copy_from_slice(&image[..7]);
    let key = infinity_figure_key(uid);
    let mut encrypted = [0; BLOCK_BYTES];
    encrypted.copy_from_slice(&image[0x10..0x20]);
    aes128_decrypt_block(&encrypted, &key)
}

pub fn infinity_figure_number(image: &[u8; INFINITY_IMAGE_BYTES]) -> u32 {
    let figure_data = decrypt_infinity_figure_data(image);
    (u32::from(figure_data[1]) << 16) | (u32::from(figure_data[2]) << 8) | u32::from(figure_data[3])
}

fn write_access_bytes(image: &mut [u8; INFINITY_IMAGE_BYTES], sector: usize, access: u32) {
    let offset = sector * 0x40 + 0x36;
    image[offset] = ((access >> 16) & 0xff) as u8;
    image[offset + 1] = ((access >> 8) & 0xff) as u8;
    image[offset + 2] = (access & 0xff) as u8;
}

fn deterministic_uid(figure_number: u32, entity_id: u32) -> [u8; 7] {
    let mut state = 0x4f4d_4e49u32 ^ figure_number.rotate_left(7) ^ entity_id.rotate_left(17);
    let mut uid = [0; 7];
    for byte in &mut uid {
        state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        *byte = (state >> 24) as u8;
    }
    if uid.iter().all(|byte| *byte == 0) {
        uid[0] = 0x04;
    }
    uid
}

fn blank_figure_data(figure_number: u32) -> [u8; BLOCK_BYTES] {
    let mut data = [0; BLOCK_BYTES];
    data[1] = ((figure_number >> 16) & 0xff) as u8;
    data[2] = ((figure_number >> 8) & 0xff) as u8;
    data[3] = (figure_number & 0xff) as u8;
    data[4] = 0x0d;
    data[5] = 0x08;
    data[6] = 0x12;
    data[9] = 0x01;
    data[10] = 0xd1;
    data[11] = 0x1f;

    let checksum = infinity_crc32_12(&data);
    data[12..16].copy_from_slice(&checksum.to_be_bytes());
    data
}

fn infinity_figure_key(uid: [u8; 7]) -> [u8; BLOCK_BYTES] {
    let mut input = [0; 38];
    input[..31].copy_from_slice(SHA1_KEY_PREFIX);
    input[31..].copy_from_slice(&uid);
    let digest = sha1_digest(&input);
    let mut key = [0; BLOCK_BYTES];
    for group in 0..4 {
        for index in 0..4 {
            key[group * 4 + index] = digest[group * 4 + (3 - index)];
        }
    }
    key
}

fn infinity_crc32_12(bytes: &[u8; BLOCK_BYTES]) -> u32 {
    let mut crc = 0u32;
    for byte in bytes.iter().take(12) {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xedb8_8320
            } else {
                crc >> 1
            };
        }
    }
    crc
}

fn sha1_digest(input: &[u8]) -> [u8; 20] {
    let bit_len = (input.len() as u64) * 8;
    let mut h0 = 0x6745_2301u32;
    let mut h1 = 0xefcd_ab89u32;
    let mut h2 = 0x98ba_dcfeu32;
    let mut h3 = 0x1032_5476u32;
    let mut h4 = 0xc3d2_e1f0u32;

    let mut block = [0; 64];
    block[..input.len()].copy_from_slice(input);
    block[input.len()] = 0x80;
    block[56..64].copy_from_slice(&bit_len.to_be_bytes());
    sha1_process_block(&block, &mut h0, &mut h1, &mut h2, &mut h3, &mut h4);

    let mut out = [0; 20];
    out[..4].copy_from_slice(&h0.to_be_bytes());
    out[4..8].copy_from_slice(&h1.to_be_bytes());
    out[8..12].copy_from_slice(&h2.to_be_bytes());
    out[12..16].copy_from_slice(&h3.to_be_bytes());
    out[16..20].copy_from_slice(&h4.to_be_bytes());
    out
}

fn sha1_process_block(
    block: &[u8; 64],
    h0: &mut u32,
    h1: &mut u32,
    h2: &mut u32,
    h3: &mut u32,
    h4: &mut u32,
) {
    let mut w = [0u32; 80];
    for (index, chunk) in block.chunks_exact(4).enumerate() {
        w[index] = u32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
    }
    for index in 16..80 {
        w[index] = (w[index - 3] ^ w[index - 8] ^ w[index - 14] ^ w[index - 16]).rotate_left(1);
    }

    let mut a = *h0;
    let mut b = *h1;
    let mut c = *h2;
    let mut d = *h3;
    let mut e = *h4;

    for (index, word) in w.iter().enumerate() {
        let (f, k) = match index {
            0..=19 => ((b & c) | ((!b) & d), 0x5a82_7999),
            20..=39 => (b ^ c ^ d, 0x6ed9_eba1),
            40..=59 => ((b & c) | (b & d) | (c & d), 0x8f1b_bcdc),
            _ => (b ^ c ^ d, 0xca62_c1d6),
        };
        let temp = a
            .rotate_left(5)
            .wrapping_add(f)
            .wrapping_add(e)
            .wrapping_add(k)
            .wrapping_add(*word);
        e = d;
        d = c;
        c = b.rotate_left(30);
        b = a;
        a = temp;
    }

    *h0 = h0.wrapping_add(a);
    *h1 = h1.wrapping_add(b);
    *h2 = h2.wrapping_add(c);
    *h3 = h3.wrapping_add(d);
    *h4 = h4.wrapping_add(e);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_contains_expected_infinity_1_entries() {
        assert_eq!(INFINITY_CATALOG.len(), 104);
        assert_eq!(INFINITY_CATALOG[0].figure_number, 0x0f4241);
        assert_eq!(INFINITY_CATALOG[0].name, "Mr. Incredible");
        assert_eq!(INFINITY_CATALOG[38].kind, FigureKind::LevelPiece);
        assert_eq!(INFINITY_CATALOG[51].figure_number, 0x2dc6cc);
    }

    #[test]
    fn generated_image_decrypts_to_dolphin_blank_figure_data() {
        let image = initialize_infinity_entity_image(0x0f4241, 7);
        assert_eq!(image.len(), INFINITY_IMAGE_BYTES);
        assert_eq!(&image[7..11], &UID_STATIC_SUFFIX);
        assert_eq!(&image[0x36..0x39], &[0x17, 0x87, 0x8e]);
        assert_eq!(&image[0x76..0x79], &[0x77, 0x87, 0x88]);

        let figure_data = decrypt_infinity_figure_data(&image);
        assert_eq!(&figure_data[1..4], &[0x0f, 0x42, 0x41]);
        assert_eq!(&figure_data[4..7], &[0x0d, 0x08, 0x12]);
        assert_eq!(&figure_data[9..12], &[0x01, 0xd1, 0x1f]);
        assert_eq!(
            u32::from_be_bytes(figure_data[12..16].try_into().unwrap()),
            infinity_crc32_12(&figure_data)
        );
        assert_eq!(infinity_figure_number(&image), 0x0f4241);
        assert_eq!(
            find_infinity_catalog_entry(infinity_figure_number(&image)).map(|entry| entry.name),
            Some("Mr. Incredible")
        );
    }
}
