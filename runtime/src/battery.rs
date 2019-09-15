use support::{decl_storage, decl_module, decl_event, StorageValue, StorageMap, dispatch::Result, ensure};
use system::ensure_signed;
use runtime_primitives::traits::Hash;
use parity_codec::{Encode, Decode};

#[derive(Encode, Decode, Default, Clone, PartialEq)]
#[cfg_attr(feature = "std", derive(Debug))]
pub struct Battery<Hash, Moment, AccountId> {
    id: Hash,
    owner: AccountId,
    station: Option<AccountId>,
    tradable: bool,
    registry_time: Moment,
}

pub trait Trait: timestamp::Trait {
    type Event: From<Event<Self>> + Into<<Self as system::Trait>::Event>;
}

decl_event!(
    pub enum Event<T>
    where
        AccountId = <T as system::Trait>::AccountId,
        Hash = <T as system::Trait>::Hash,
    {
        RegistryStation(AccountId),
        RegistryBattery(AccountId, Hash, AccountId),
        SwitchTradable(Hash, bool),
        StoreToStation(Hash, AccountId, AccountId),
        FetchFromStation(Hash, AccountId, AccountId),
        Trade(Hash, AccountId, AccountId, AccountId),
    }
);

decl_storage! {
    trait Store for Module<T: Trait> as Battery {

        Batteries get(batteries): map T::Hash => Battery<T::Hash, T::Moment, T::AccountId>;
        
        AllBatteriesCount get(all_batteries_count): u64;
        AllBatteriesArray get(battery_by_index): map u64 => T::Hash;
        
        OwnedBatteriesCount get(owned_batteries_count): map T::AccountId => u64;
        OwnedBatteriesArray get(battery_of_owner_by_index): map (T::AccountId, u64) => T::Hash;
        OwnedBatteriesIndex get(owned_battery_index): map T::Hash => u64;

        StationsCount get(stations_count): u64;
        StationsArray get(station_by_index): map u64 => T::AccountId;
        StationsIndex get(station_index): map T::AccountId => u64;

        BatteriesCountInStation get(batteries_count_in_station): map T::AccountId => u64;
        BatteriesArrayInStation get(battery_of_station_by_index): map (T::AccountId, u64) => T::Hash;
        BatteriesIndexInStation get(battery_index_in_station): map T::Hash => u64;
    }
}

decl_module! {
    pub struct Module<T: Trait> for enum Call where origin: T::Origin {
        fn deposit_event<T>() = default;

        pub fn register_station(origin) -> Result {
            let sender = ensure_signed(origin)?;
            ensure!(!<StationsIndex<T>>::exists(sender.clone()), "Already been station!");

            <StationsArray<T>>::insert(Self::stations_count(), sender.clone());
            <StationsIndex<T>>::insert(sender.clone(), Self::stations_count());
            <StationsCount<T>>::mutate(|n| *n += 1);

            Self::deposit_event(RawEvent::RegistryStation(sender));
            Ok(())
        }

        pub fn registry_battery(origin, owner: T::AccountId) -> Result {
            let sender = ensure_signed(origin)?;
            ensure!(<StationsIndex<T>>::exists(sender.clone()), "Not a station!");

            let payload = (
                <system::Module<T>>::random_seed(), 
                &owner,
                <system::Module<T>>::extrinsic_index(),
                <system::Module<T>>::block_number(),
                Self::all_batteries_count(),
            );
            let random_hash = payload.using_encoded(<T as system::Trait>::Hashing::hash);

            ensure!(!<Batteries<T>>::exists(random_hash), "Battery already exists!");
            let new_battery = Battery {
                id: random_hash,
                owner: owner.clone(),
                station: Some(sender.clone()),
                tradable: false,
                registry_time: <timestamp::Module<T>>::get(),
            };

            // change state
            <Batteries<T>>::insert(random_hash, new_battery);
            <AllBatteriesArray<T>>::insert(Self::all_batteries_count(), random_hash);
            <AllBatteriesCount<T>>::mutate(|n| *n += 1);
            <OwnedBatteriesArray<T>>::insert((owner.clone(), Self::owned_batteries_count(owner.clone())), random_hash);
            <OwnedBatteriesIndex<T>>::insert(random_hash, Self::owned_batteries_count(owner.clone()));
            <OwnedBatteriesCount<T>>::mutate(owner.clone(), |n| *n += 1);
            <BatteriesArrayInStation<T>>::insert((sender.clone(), Self::batteries_count_in_station(sender.clone())), random_hash);
            <BatteriesIndexInStation<T>>::insert(random_hash, Self::batteries_count_in_station(sender.clone()));
            <BatteriesCountInStation<T>>::mutate(sender.clone(), |n| *n += 1);

            Self::deposit_event(RawEvent::RegistryBattery(sender, random_hash, owner));
            Ok(())
        }

        pub fn switch_tradable(origin, id: T::Hash) -> Result {
            let sender = ensure_signed(origin)?;

            ensure!(<Batteries<T>>::exists(id), "Id does not exist");
            let mut battery = Self::batteries(id);
            ensure!(battery.owner == sender, "You are not the owner of this battery");
            ensure!(battery.station != None, "Battery must be in station");
            battery.tradable = !battery.tradable;

            <Batteries<T>>::insert(id, battery.clone());

            Self::deposit_event(RawEvent::SwitchTradable(id, battery.tradable));
            Ok(())
        }

        pub fn store_to_station(origin, id: T::Hash) -> Result {
            let sender = ensure_signed(origin)?;

            ensure!(<StationsIndex<T>>::exists(sender.clone()), "Sender is not a station");
            ensure!(<Batteries<T>>::exists(id), "Battery does not exist");
            let mut battery = Self::batteries(id);
            ensure!(battery.station == None, "Station of the battery must be None");
            battery.station = Some(sender.clone());

            // change state
            <Batteries<T>>::insert(id, battery.clone());
            <BatteriesArrayInStation<T>>::insert((sender.clone(), Self::batteries_count_in_station(sender.clone())), id);
            <BatteriesIndexInStation<T>>::insert(id, Self::batteries_count_in_station(sender.clone()));
            <BatteriesCountInStation<T>>::mutate(sender.clone(), |n| *n += 1);

            Self::deposit_event(RawEvent::StoreToStation(id, battery.owner, sender));
            Ok(())
        }

        pub fn fetch_from_station(origin, id: T::Hash) -> Result {
            let sender = ensure_signed(origin)?;

            ensure!(<Batteries<T>>::exists(id), "Battery does not exist");
            let mut battery = Self::batteries(id);
            ensure!(battery.owner == sender, "You are not the owner of this battery");
            let station = battery.station.ok_or("No station for this battery")?;
            battery.station = None;
            battery.tradable = false;

            let battery_index = Self::battery_index_in_station(id);
            let batteries_count = Self::batteries_count_in_station(station.clone());

            // not the last one of the station
            if batteries_count != battery_index + 1 {
                let last_battery_id = Self::battery_of_station_by_index((station.clone(), batteries_count - 1));
                <BatteriesArrayInStation<T>>::insert((station.clone(), battery_index), last_battery_id);
                <BatteriesIndexInStation<T>>::insert(last_battery_id, battery_index);
            }
            <BatteriesArrayInStation<T>>::remove((station.clone(), batteries_count - 1));
            <BatteriesIndexInStation<T>>::remove(id);
            <BatteriesCountInStation<T>>::mutate(station.clone(), |n| *n -= 1);
            <Batteries<T>>::insert(id, battery.clone());

            Self::deposit_event(RawEvent::FetchFromStation(id, sender, battery.owner));
            Ok(())
        }

        pub fn trade_battery(origin, id: T::Hash, to: T::AccountId) -> Result {
            let sender = ensure_signed(origin)?;

            ensure!(<StationsIndex<T>>::exists(sender.clone()), "Sender is not a station");
            ensure!(<Batteries<T>>::exists(id), "Battery does not exist");
            let mut battery = Self::batteries(id);
            ensure!(battery.station == Some(sender.clone()), "Sender must be the station of this battery");
            ensure!(battery.tradable, "Battery must be tradable");
            let from = battery.owner.clone();
            ensure!(from != to, "To account can't be the owner of this battery");
            battery.owner = to.clone();
            battery.tradable = false;

            // change state
            let owned_battery_count_from = Self::owned_batteries_count(from.clone());
            let owned_battery_count_to = Self::owned_batteries_count(to.clone());
            let new_owned_battery_count_from = owned_battery_count_from - 1;
            let new_owned_battery_count_to = owned_battery_count_to + 1;

            let owned_battery_index = Self::owned_battery_index(id);
            if owned_battery_index != new_owned_battery_count_from {
                let last_battery_id_from = Self::battery_of_owner_by_index((from.clone(), owned_battery_index));
                <OwnedBatteriesArray<T>>::insert((from.clone(), owned_battery_index), last_battery_id_from);
                <OwnedBatteriesIndex<T>>::insert(last_battery_id_from, owned_battery_index);
            }
            <Batteries<T>>::insert(id, battery);
            <OwnedBatteriesIndex<T>>::insert(id, owned_battery_count_to);
            <OwnedBatteriesArray<T>>::remove((from.clone(), new_owned_battery_count_from));
            <OwnedBatteriesArray<T>>::insert((to.clone(), owned_battery_count_to), id);
            <OwnedBatteriesCount<T>>::insert(from.clone(), new_owned_battery_count_from);
            <OwnedBatteriesCount<T>>::insert(to.clone(), new_owned_battery_count_to);

            Self::deposit_event(RawEvent::Trade(id, from, to, sender));
            Ok(())
        }
    }
}