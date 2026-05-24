pub type UserId = u32;
pub type GroupId = u32;

pub const ROOT_UID: UserId = 0;
pub const ROOT_GID: GroupId = 0;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Credentials {
    pub uid: UserId,
    pub gid: GroupId,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FileMode(pub u16);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FileMetadata {
    pub owner: UserId,
    pub group: GroupId,
    pub mode: FileMode,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Access {
    Read,
    Write,
    Execute,
}

impl Credentials {
    pub const fn root() -> Self {
        Self {
            uid: ROOT_UID,
            gid: ROOT_GID,
        }
    }

    pub const fn user(uid: UserId, gid: GroupId) -> Self {
        Self { uid, gid }
    }

    pub const fn is_superuser(&self) -> bool {
        self.uid == ROOT_UID
    }
}

impl FileMode {
    pub const fn new(bits: u16) -> Self {
        Self(bits)
    }

    fn bit(self, access: Access, shift: u16) -> bool {
        let bit = match access {
            Access::Read => 0o4,
            Access::Write => 0o2,
            Access::Execute => 0o1,
        };
        self.0 & (bit << shift) != 0
    }
}

impl FileMetadata {
    pub const fn new(owner: UserId, group: GroupId, mode: u16) -> Self {
        Self {
            owner,
            group,
            mode: FileMode::new(mode),
        }
    }

    pub fn can_access(&self, creds: Credentials, access: Access) -> bool {
        if creds.is_superuser() {
            return true;
        }

        if creds.uid == self.owner {
            return self.mode.bit(access, 6);
        }

        if creds.gid == self.group {
            return self.mode.bit(access, 3);
        }

        self.mode.bit(access, 0)
    }
}

pub fn init() {
    self_test();
}

pub fn self_test() {
    let private = FileMetadata::new(ROOT_UID, ROOT_GID, 0o600);
    let executable = FileMetadata::new(1000, 1000, 0o755);
    let user = Credentials::user(1000, 1000);

    if private.can_access(user, Access::Read) {
        panic!("permission self-test allowed user to read root private file");
    }
    if !private.can_access(Credentials::root(), Access::Write) {
        panic!("permission self-test denied root write");
    }
    if !executable.can_access(user, Access::Execute) {
        panic!("permission self-test denied executable bit");
    }

    crate::println!("Permissions self-test passed.");
}

