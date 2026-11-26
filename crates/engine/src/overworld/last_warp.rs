//! Classification of the last-used warp's map.

use assets::MapId;

/// Upstream's `sPokemonCenters` members, excluding its `MAP_UNDEFINED` terminator.
const POKEMON_CENTER_MAPS: [MapId; 18] = [
    MapId("MAP_OLDALE_TOWN_POKEMON_CENTER_1F"),
    MapId("MAP_DEWFORD_TOWN_POKEMON_CENTER_1F"),
    MapId("MAP_LAVARIDGE_TOWN_POKEMON_CENTER_1F"),
    MapId("MAP_FALLARBOR_TOWN_POKEMON_CENTER_1F"),
    MapId("MAP_VERDANTURF_TOWN_POKEMON_CENTER_1F"),
    MapId("MAP_PACIFIDLOG_TOWN_POKEMON_CENTER_1F"),
    MapId("MAP_PETALBURG_CITY_POKEMON_CENTER_1F"),
    MapId("MAP_SLATEPORT_CITY_POKEMON_CENTER_1F"),
    MapId("MAP_MAUVILLE_CITY_POKEMON_CENTER_1F"),
    MapId("MAP_RUSTBORO_CITY_POKEMON_CENTER_1F"),
    MapId("MAP_FORTREE_CITY_POKEMON_CENTER_1F"),
    MapId("MAP_LILYCOVE_CITY_POKEMON_CENTER_1F"),
    MapId("MAP_MOSSDEEP_CITY_POKEMON_CENTER_1F"),
    MapId("MAP_SOOTOPOLIS_CITY_POKEMON_CENTER_1F"),
    MapId("MAP_EVER_GRANDE_CITY_POKEMON_CENTER_1F"),
    MapId("MAP_EVER_GRANDE_CITY_POKEMON_LEAGUE_1F"),
    MapId("MAP_BATTLE_FRONTIER_POKEMON_CENTER_1F"),
    MapId("MAP_UNION_ROOM"),
];

/// `UsedPokemonCenterWarp` (`pokeemerald/src/field_specials.c:2008-2043`).
///
/// `last_used_warp_map` is the map of the last-used warp, not the current map
/// or the saved heal destination.
///
/// Returns `true` when that map is in upstream's fixed set: the 1F map of each
/// Pokémon Center, the Ever Grande Pokémon League 1F and the Union Room. Any
/// other map, including Pokémon Center 2F maps, returns `false`.
#[must_use]
pub fn used_pokemon_center_warp(last_used_warp_map: MapId) -> bool {
    POKEMON_CENTER_MAPS.contains(&last_used_warp_map)
}

#[cfg(test)]
mod tests {
    use assets::MapHeaderTable;

    use super::*;

    const MEMBERS: [(&str, u8, u8); 18] = [
        ("MAP_OLDALE_TOWN_POKEMON_CENTER_1F", 2, 2),
        ("MAP_DEWFORD_TOWN_POKEMON_CENTER_1F", 3, 1),
        ("MAP_LAVARIDGE_TOWN_POKEMON_CENTER_1F", 4, 5),
        ("MAP_FALLARBOR_TOWN_POKEMON_CENTER_1F", 5, 4),
        ("MAP_VERDANTURF_TOWN_POKEMON_CENTER_1F", 6, 4),
        ("MAP_PACIFIDLOG_TOWN_POKEMON_CENTER_1F", 7, 0),
        ("MAP_PETALBURG_CITY_POKEMON_CENTER_1F", 8, 4),
        ("MAP_SLATEPORT_CITY_POKEMON_CENTER_1F", 9, 11),
        ("MAP_MAUVILLE_CITY_POKEMON_CENTER_1F", 10, 5),
        ("MAP_RUSTBORO_CITY_POKEMON_CENTER_1F", 11, 5),
        ("MAP_FORTREE_CITY_POKEMON_CENTER_1F", 12, 2),
        ("MAP_LILYCOVE_CITY_POKEMON_CENTER_1F", 13, 6),
        ("MAP_MOSSDEEP_CITY_POKEMON_CENTER_1F", 14, 3),
        ("MAP_SOOTOPOLIS_CITY_POKEMON_CENTER_1F", 15, 2),
        ("MAP_EVER_GRANDE_CITY_POKEMON_CENTER_1F", 16, 12),
        ("MAP_EVER_GRANDE_CITY_POKEMON_LEAGUE_1F", 16, 10),
        ("MAP_BATTLE_FRONTIER_POKEMON_CENTER_1F", 26, 53),
        ("MAP_UNION_ROOM", 25, 60),
    ];

    #[test]
    fn every_upstream_member_is_a_pokemon_center_warp() {
        for (name, _, _) in MEMBERS {
            assert!(used_pokemon_center_warp(MapId(name)), "{name}");
        }
    }

    #[test]
    fn members_sit_at_their_upstream_group_and_number() {
        let headers = MapHeaderTable::new();
        for (name, group, num) in MEMBERS {
            let header = headers.get_by_position(group, num).expect(name);
            assert_eq!(header.id, MapId(name));
        }
    }

    #[test]
    fn terminator_and_unknown_ids_are_not_members() {
        assert!(!used_pokemon_center_warp(MapId("MAP_UNDEFINED")));
        assert!(!used_pokemon_center_warp(MapId("")));
        assert!(!used_pokemon_center_warp(MapId("MAP_NOT_A_MAP")));
    }

    #[test]
    fn second_floors_and_neighbouring_maps_are_not_members() {
        for name in [
            "MAP_OLDALE_TOWN_POKEMON_CENTER_2F",
            "MAP_BATTLE_FRONTIER_POKEMON_CENTER_2F",
            "MAP_EVER_GRANDE_CITY_POKEMON_CENTER_2F",
            "MAP_MOSSDEEP_CITY_SPACE_CENTER_1F",
            "MAP_OLDALE_TOWN",
            "MAP_ROUTE101",
            "MAP_LITTLEROOT_TOWN",
            "MAP_TRADE_CENTER",
        ] {
            assert!(!used_pokemon_center_warp(MapId(name)), "{name}");
        }
    }

    #[test]
    fn maps_adjacent_to_members_by_position_are_not_members() {
        let headers = MapHeaderTable::new();
        for (name, group, num) in MEMBERS {
            for neighbour in [num.wrapping_sub(1), num + 1] {
                let Some(header) = headers.get_by_position(group, neighbour) else {
                    continue;
                };
                if MEMBERS.iter().any(|&(member, _, _)| member == header.id.0) {
                    continue;
                }
                assert!(!used_pokemon_center_warp(header.id), "{name} neighbour");
            }
        }
    }

    #[test]
    fn swapped_group_and_number_positions_match_only_where_they_are_members() {
        let headers = MapHeaderTable::new();
        let mut non_members = 0;
        for (name, group, num) in MEMBERS {
            let Some(header) = headers.get_by_position(num, group) else {
                continue;
            };
            let is_member = MEMBERS.contains(&(header.id.0, num, group));
            assert_eq!(used_pokemon_center_warp(header.id), is_member, "{name}");
            non_members += usize::from(!is_member);
        }
        assert!(non_members > 0);
    }

    #[test]
    fn no_route_is_a_member() {
        for header in MapHeaderTable::new().iter() {
            if header.id.0.starts_with("MAP_ROUTE") {
                assert!(!used_pokemon_center_warp(header.id), "{}", header.id.0);
            }
        }
    }

    #[test]
    fn only_the_eighteen_members_match_across_all_headers() {
        let matches = MapHeaderTable::new()
            .iter()
            .filter(|header| used_pokemon_center_warp(header.id))
            .count();
        assert_eq!(matches, MEMBERS.len());
    }
}
