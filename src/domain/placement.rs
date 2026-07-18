#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PortalPlacement {
    Skylanders(SkylandersPlacement),
    Infinity(InfinityPlacement),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SkylandersPlacement {
    pub portal_index: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InfinityPlacement {
    Figure,
    PowerDiscOne,
    PowerDiscTwo,
}
