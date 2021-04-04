use rg3d::core::pool::Handle;

use crate::weapon::Weapon;

pub enum Message {
    ShootWeapon { weapon: Handle<Weapon> },
}
