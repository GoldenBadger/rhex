use std::collections::{HashMap, HashSet};
use std::collections::hash_map::Entry;
use std::sync::{Arc};

use hex2dext::algo::bfs;
use hex2d::{Coordinate, Direction, Angle, Position};
use hex2d::Angle::{Left, Right, Forward};

use actor::{self, Race, NoiseType};
use generate;
use hex2dext::algo;
use item::Item;
use util::random_pos;

use self::tile::{Tile, Feature};

pub mod area;
pub mod tile;
pub mod controller;

pub use self::controller::Controller;

pub type Map = HashMap<Coordinate, tile::Tile>;
pub type Actors = HashMap<Coordinate, actor::State>;
pub type Items = HashMap<Coordinate, Box<Item>>;
pub type LightMap = HashMap<Coordinate, u32>;

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum Action {
    Wait,
    Turn(Angle),
    Move(Angle),
    Spin(Angle),
    Equip(char),
    Fire(Coordinate),
    Pick,
    Descend,
}

#[derive(Clone, Debug)]
pub struct State {
    pub actors: HashMap<u32, actor::State>, // id -> State
    pub actors_pos: HashMap<Coordinate, u32>, // coord -> id
    pub actors_dead : HashSet<u32>,
    pub actors_counter : u32,
    pub map : Arc<Map>,
    pub items: Items,
    pub light_map: LightMap,
    pub turn : u64,
    pub descend : bool,
    pub level : i32,
}

pub fn action_could_be_attack(action : Action) -> bool {
    match action {
        Action::Move(angle) => match angle {
            Left|Right|Forward => true,
            _ => false,
        },
        _ => false,
    }
}

impl State {
    pub fn new() -> State {

        let cp = Coordinate::new(0, 0);
        let (map, gen_actors, items) = generate::DungeonGenerator::new(0).generate_map(cp, 400);

        let mut actors = HashMap::new();
        let mut actors_pos = HashMap::new();

        let mut actors_counter = 0;

        for (coord, astate) in gen_actors {
            actors_pos.insert(coord, actors_counter);
            actors.insert(actors_counter, astate);
            actors_counter += 1;
        }

        let mut state = State {
            actors: actors,
            actors_pos: actors_pos,
            actors_counter: actors_counter,
            actors_dead: HashSet::new(),
            items: items,
            map: Arc::new(map),
            turn: 0,
            level: 0,
            descend: false,
            light_map: HashMap::new(),
        };

        state.spawn_player(random_pos(0, 0));
        //state.spawn_pony(random_pos(-1, 0));

        state
    }

    pub fn next_level(&self) -> State {
        let cp = Coordinate::new(0, 0);
        let (map, gen_actors, items) = generate::DungeonGenerator::new(self.level + 1).generate_map(cp, 400);

        let mut actors = HashMap::new();
        let mut actors_pos = HashMap::new();

        let mut actors_counter = 0;

        for (coord, astate) in gen_actors {
            actors_pos.insert(coord, actors_counter);
            actors.insert(actors_counter, astate);
            actors_counter += 1;
        }

        let mut player = None;
        let mut pony = None;

        for (_, astate) in self.actors.iter() {
            if astate.is_player() {
                player = Some(astate.clone());
                break;
            }
        }

        for (_, astate) in self.actors.iter() {
            if astate.race == Race::Pony {
                pony = Some(astate.clone());
                break;
            }
        }

        let mut state = State {
            actors: actors,
            actors_pos: actors_pos,
            actors_counter: actors_counter,
            actors_dead: HashSet::new(),
            items: items,
            map: Arc::new(map),
            turn: self.turn,
            descend: false,
            level: self.level + 1,
            light_map: HashMap::new(),
        };

        {
            let mut player = player.unwrap();
            let pos = random_pos(0, 0);
            player.moved(pos);
            player.changed_level();
            state.spawn(player);
        }

        if let Some(mut pony) = pony {
            let pos = random_pos(-1, 0);
            pony.moved(pos);
            pony.changed_level();
            state.spawn(pony);
        }

        state
    }

    pub fn recalculate_noise(&mut self) {
        for id in self.actors_alive_ids() {
            let source_emission = self.actors[id].noise_emision;
            if source_emission > 0 {
                let source_race = self.actors[id].race;
                let source_coord = self.actors[id].pos.coord;
                source_coord.for_each_in_range(source_emission, |coord| {
                    if let Some(&target_id) = self.actors_pos.get(&coord) {
                        self.actors[target_id].noise_hears(source_coord, NoiseType::Creature(source_race));
                    }
                });
            }
        }
    }

    pub fn actors_ids(&self) -> Vec<u32> {
        self.actors.keys().cloned().collect()
    }

    pub fn actors_alive_ids(&self) -> Vec<u32> {
        self.actors.keys().filter(|&id| !self.actors[*id].is_dead()).cloned().collect()
    }

    pub fn recalculate_light_map(&mut self) {
        let mut light_map : HashMap<Coordinate, u32> = HashMap::new();

        for (pos, tile) in &*self.map {
            let light = tile.light;
            if light > 0 {
                algo::los::los(
                    &|coord| {
                        if coord == *pos {
                            0
                        } else {
                            self.at(coord).tile_map_or(light, |tile| tile.opaqueness())
                        }
                    },
                    &mut |coord, light| {
                        match light_map.entry(coord) {
                            Entry::Occupied(mut entry) => {
                                let val = entry.get_mut();
                                if light as u32 > *val {
                                    *val = light as u32;
                                }
                            },
                            Entry::Vacant(entry) => {
                                entry.insert(light as u32);
                            },
                        }
                    },
                    light, *pos, Direction::all()
                    );
            }
        }

        for (_, &id) in &self.actors_pos {
            let astate = &self.actors[id];
            let pos = astate.pos.coord;
            if astate.light_emision > 0 {
                algo::los::los(
                    &|coord| {
                        if coord == pos {
                            0
                        } else {
                            self.at(coord).tile_map_or(astate.light_emision as i32, |tile| tile.opaqueness())
                        }
                    },
                    &mut |coord, light| {
                        match light_map.entry(coord) {
                            Entry::Occupied(mut entry) => {
                                let val = entry.get_mut();
                                if light as u32 > *val {
                                    *val = light as u32;
                                }
                            },
                            Entry::Vacant(entry) => {
                                entry.insert(light as u32);
                            },
                        }
                    },
                    astate.light_emision as i32, pos, Direction::all()
                );
            }
        }

        self.light_map = light_map;
    }

    pub fn spawn(&mut self, astate : actor::State) {
        let id = self.actors_counter;
        self.actors_counter += 1;

        self.actors_pos.insert(astate.pos.coord, id);
        self.actors.insert(id, astate);
    }

    pub fn spawn_player(&mut self, pos : Position) {
        let mut actor = actor::State::new(actor::Race::Human, pos);
        actor.set_player();
        self.spawn(actor)
    }


    pub fn act(&mut self, id : u32, action : Action) {

        if !self.actors[id].can_perform_action() {
            return;
        }

        let old_pos = self.actors[id].pos;
        let new_pos = self.actors[id].pos_after_action(action);

        if self.actors[id].pos == new_pos {
            // no movement
            match action {
                Action::Pick => {
                    let head = self.actors[id].head();
                    let item = self.at_mut(head).pick_item();

                    match item {
                        Some(item) => {
                            self.actors[id].add_item(item);
                        },
                        None => {},
                    }
                },
                Action::Equip(ch) => {
                    self.actors[id].equip_switch(ch);
                },
                Action::Descend => {
                    if self.at(self.actors[id].coord()).tile_map_or(false, |t| t.feature == Some(Feature::Stairs)) {
                        self.descend = true;
                    }
                },
                _ => {}
            }
        } else if action_could_be_attack(action) &&
            old_pos.coord != new_pos.coord &&
            self.actors_pos.contains_key(&new_pos.coord)
            {
            // we've tried to move into actor; attack?
            if !self.actors[id].can_attack() {
                return;
            }
            let dir = match action {
                Action::Move(dir) => old_pos.dir + dir,
                _ => old_pos.dir,
            };

            let target_id = self.actors_pos[new_pos.coord];

            let mut target = self.actors.remove(&target_id).unwrap();
            self.actors[id].attacks(dir, &mut target);
            self.actors.insert(target_id, target);

        } else if self.at(new_pos.coord).tile_map_or(
            false, |t| t.feature == Some(tile::Door(false))
            ) {
            // walked into door: open it
            let mut map = self.map.clone().make_unique().clone();
            let tile = map.remove(&new_pos.coord).unwrap();
            map.insert(new_pos.coord, tile.add_feature(tile::Door(true)));
            self.map = Arc::new(map);

        } else if old_pos.coord == new_pos.coord || self.at(new_pos.coord).is_passable() {
            // we've moved
            self.actors[id].moved(new_pos);
            // we will remove the previous position on post_tick, so that
            // for the rest of this turn this actor can be found through both new
            // and old coor
            self.actors_pos.insert(new_pos.coord, id);
        } else {
            // we hit the wall or something
        }
    }

    pub fn pre_tick(&mut self) {
        for id in self.actors_alive_ids() {
            let mut actor = self.actors.remove(&id).unwrap();
            actor.pre_tick(self);
            self.actors.insert(id, actor);
        }
    }

    /// Advance one turn (increase the turn counter) and do some maintenance
    pub fn post_tick(&mut self) {

        for id in self.actors_ids() {
            if self.actors[id].is_dead() && !self.actors_dead.contains(&id){
                let mut a = self.actors.remove(&id).unwrap();

                for (_, item) in a.items_backpack.drain() {
                    self.at_mut(a.pos.coord).drop_item(item);
                }

                for (_, (_, item)) in a.items_equipped.drain() {
                    self.at_mut(a.pos.coord).drop_item(item);
                }
                self.actors.insert(id, a);

                self.actors_dead.insert(id);
            }
        }

        self.actors_pos = self.actors_pos.iter().filter(|&(coord, ref id)|
                                                 !self.actors[**id].is_dead() && (self.actors[**id].coord() == *coord)
                                                ).map(|(coord, id)| (*coord, *id)).collect();

        self.recalculate_light_map();
        self.recalculate_noise();

        for id in self.actors_alive_ids() {
            let mut actor = self.actors.remove(&id).unwrap();
            actor.post_tick(self);
            self.actors.insert(id, actor);
        }

        self.turn += 1;
    }

    pub fn at(&self, coord: Coordinate) -> At {
        At {
            coord: coord,
            state: self
        }
    }

    pub fn at_mut(&mut self, coord: Coordinate) -> AtMut {
        AtMut {
            coord: coord,
            state: self
        }
    }


}

pub struct At<'a> {
    coord : Coordinate,
    state : &'a State,
}

impl<'a> At<'a> {
    pub fn tile(&self) -> Option<&'a tile::Tile> {
        self.state.map.get(&self.coord)
    }

    pub fn tile_map_or<R, F>(&self, def: R, f : F) -> R
        where F : Fn(&tile::Tile) -> R
    {
        self.state.map.get(&self.coord).map_or(def, |a| f(a))
    }

    pub fn actor_map_or<R, F : Fn(&actor::State) -> R>
        (&self, def: R, cond : F) -> R
    {
        self.state.actors_pos.get(&self.coord).map(|&id| &self.state.actors[id]).map_or(def, |a| cond(&a))
    }

    pub fn item_map_or<R, F : Fn(&Box<Item>) -> R>
        (&self, def: R, cond : F) -> R
    {
        self.state.items.get(&self.coord).map_or(def, |i| cond(i))
    }

    pub fn is_occupied(&self) -> bool {
        self.state.actors_pos.contains_key(&self.coord)
    }

    pub fn is_passable(&self) -> bool {
        !self.is_occupied() && self.tile_map_or(false, |t| t.is_passable())
    }

    pub fn light(&self) -> u32 {
        self.state.light_map.get(&self.coord).map_or(0, |l| *l)
    }

    pub fn item(&self) -> Option<&'a Item> {
        self.state.items.get(&self.coord).map(|i| &**i)
    }
}

pub struct AtMut<'a> {
    coord : Coordinate,
    state : &'a mut State,
}

impl<'a> AtMut<'a> {
    /*
    pub fn to_at(&'a self) -> At<'a> {
        At {
            coord: self.coord,
            state: self.state
        }
    }*/

    pub fn drop_item(&mut self, item : Box<Item>) {
        let coord = {
            let mut bfs = bfs::Traverser::new(
                |coord| self.state.at(coord).tile_map_or(false, |t| t.is_passable()),
                |coord| self.state.at(coord).tile_map_or(false, |t| t.is_passable()) && self.state.items.get(&coord).is_none(),
                self.coord
                );

            bfs.find()
        };

        match coord {
            None => { /* destroy the item :/ */ },
            Some(coord) => {
                self.state.items.insert(coord, item);
            }
        }
    }

    pub fn pick_item(&mut self) -> Option<Box<Item>> {
        if self.state.items.get(&self.coord).is_some() {
            self.state.items.remove(&self.coord)
        } else {
            None
        }
    }
}
