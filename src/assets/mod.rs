#[allow(unused)]
pub mod jar;
#[allow(unused)]
pub mod tgam;

use std::fmt::Debug;
use std::marker::PhantomData;

use bevy::asset::AssetIo;
use bevy::prelude::*;

pub struct CustomAssetIoPlugin<IO, P>(P, PhantomData<IO>);

impl<IO, P> CustomAssetIoPlugin<IO, P> {
    fn new(props: P) -> Self {
        CustomAssetIoPlugin(props, PhantomData)
    }
}

impl<IO, P> Plugin for CustomAssetIoPlugin<IO, P>
where
    IO: AssetIo + TryFrom<AssetIoProps<P>> + Sync + Send + 'static,
    IO::Error: Debug,
    P: Clone + Sync + Send + 'static,
{
    fn build(&self, app: &mut App) {
        let task_pool = app
            .world
            .get_resource::<bevy::tasks::IoTaskPool>()
            .expect("`IoTaskPool` resource not found.")
            .0
            .clone();

        let base = bevy::asset::create_platform_default_asset_io(app);
        let props = AssetIoProps {
            base,
            props: self.0.clone(),
        };
        let source = IO::try_from(props).expect("could not initialize asset IO");
        app.insert_resource(AssetServer::new(source, task_pool));
    }
}

pub struct AssetIoProps<P> {
    base: Box<dyn AssetIo>,
    props: P,
}
