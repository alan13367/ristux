use std::fmt;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub struct Error;
impl Error {
    pub fn from_str(_: &str) -> Self {
        Self
    }
    pub fn class(&self) -> ErrorClass {
        ErrorClass::Callback
    }
    pub fn code(&self) -> ErrorCode {
        ErrorCode::NotFound
    }
    pub fn message(&self) -> &str {
        "Ristux offline Git unavailable"
    }
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Ristux offline Git unavailable")
    }
}
impl std::error::Error for Error {}

pub struct Repository {
    inner: gix::Repository,
    git_dir: PathBuf,
    workdir: Option<PathBuf>,
}
pub struct Config;
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Oid([u8; 20]);
pub struct Object<'a> {
    repo: &'a Repository,
    id: Oid,
    blob: Option<Blob>,
}
pub struct Tree<'a> {
    repo: &'a Repository,
    id: Oid,
}
pub struct Submodule<'a>(std::marker::PhantomData<&'a ()>);
pub struct Buf(String);
pub struct FetchOptions<'a>(std::marker::PhantomData<&'a ()>);
pub struct RemoteCallbacks<'a>(std::marker::PhantomData<&'a ()>);
pub struct RepositoryInitOptions {
    bare: bool,
}
pub struct StatusOptions;
pub type Credentials<'a> =
    dyn FnMut(&str, Option<&str>, CredentialType) -> Result<Cred, Error> + 'a;
pub struct CredentialHelper {
    pub username: Option<String>,
}
pub struct Cred;
pub struct Version;

pub enum AutotagOption {
    All,
}
pub enum BranchType {
    Remote,
}
pub enum CertificateCheckStatus {
    CertificateOk,
    CertificatePassthrough,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ErrorClass {
    Http,
    Net,
    Os,
    Zlib,
    Ssl,
    Submodule,
    FetchHead,
    Ssh,
    Callback,
    Reference,
    Odb,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ErrorCode {
    Certificate,
    NotFound,
}
pub enum ObjectType {
    Commit,
}
pub enum ResetType {
    Hard,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SubmoduleUpdate {
    None,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Status(u32);
impl Status {
    pub const CURRENT: Self = Self(0);
    pub const INDEX_DELETED: Self = Self(1);
    pub const INDEX_MODIFIED: Self = Self(2);
    pub const INDEX_NEW: Self = Self(4);
    pub const INDEX_RENAMED: Self = Self(8);
    pub const INDEX_TYPECHANGE: Self = Self(16);
}

#[derive(Clone, Copy)]
pub struct CredentialType(u32);
impl CredentialType {
    pub const DEFAULT: Self = Self(1);
    pub const SSH_KEY: Self = Self(2);
    pub const USERNAME: Self = Self(4);
    pub const USER_PASS_PLAINTEXT: Self = Self(8);
}
impl CredentialType {
    pub fn contains(self, other: Self) -> bool {
        self.0 & other.0 != 0
    }
}

impl fmt::Display for Oid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.0 {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}
impl std::str::FromStr for Oid {
    type Err = Error;
    fn from_str(value: &str) -> Result<Self, Error> {
        let id: gix::ObjectId = value.parse().map_err(|_| Error)?;
        Ok(Self::from_gix(id))
    }
}
impl Oid {
    pub fn from_str(value: &str) -> Result<Self, Error> {
        value.parse()
    }
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    fn from_gix(id: gix::ObjectId) -> Self {
        let mut bytes = [0; 20];
        bytes.copy_from_slice(id.as_slice());
        Self(bytes)
    }

    fn into_gix(self) -> gix::ObjectId {
        self.0.into()
    }
}
impl Buf {
    pub fn as_str(&self) -> Option<&str> {
        Some(&self.0)
    }
}

impl Config {
    pub fn new() -> Result<Self, Error> {
        Ok(Self)
    }
    pub fn open_default() -> Result<Self, Error> {
        Ok(Self)
    }
    pub fn get_string(&self, _: &str) -> Result<String, Error> {
        Err(Error)
    }
    pub fn get_path(&self, _: &str) -> Result<std::path::PathBuf, Error> {
        Err(Error)
    }
    pub fn set_bool(&mut self, _: &str, _: bool) -> Result<(), Error> {
        Ok(())
    }
}
impl FetchOptions<'_> {
    pub fn new() -> Self {
        Self(std::marker::PhantomData)
    }
    pub fn remote_callbacks(&mut self, _: RemoteCallbacks<'_>) -> &mut Self {
        self
    }
    pub fn download_tags(&mut self, _: AutotagOption) -> &mut Self {
        self
    }
    pub fn depth(&mut self, _: i32) -> &mut Self {
        self
    }
}
impl RemoteCallbacks<'_> {
    pub fn new() -> Self {
        Self(std::marker::PhantomData)
    }
    pub fn credentials<F>(&mut self, _: F) -> &mut Self
    where
        F: FnMut(&str, Option<&str>, CredentialType) -> Result<Cred, Error>,
    {
        self
    }
    pub fn certificate_check<F>(&mut self, _: F) -> &mut Self
    where
        F: FnMut(&cert::Cert<'_>, &str) -> Result<CertificateCheckStatus, Error>,
    {
        self
    }
    pub fn transfer_progress<F>(&mut self, _: F) -> &mut Self
    where
        F: FnMut(Progress<'_>) -> bool,
    {
        self
    }
}
impl RepositoryInitOptions {
    pub fn new() -> Self {
        Self { bare: false }
    }
    pub fn external_template(&mut self, _: bool) -> &mut Self {
        self
    }
    pub fn bare(&mut self, bare: bool) -> &mut Self {
        self.bare = bare;
        self
    }
    pub fn no_reinit(&mut self, _: bool) -> &mut Self {
        self
    }
    pub fn mkdir(&mut self, _: bool) -> &mut Self {
        self
    }
}
impl StatusOptions {
    pub fn new() -> Self {
        Self
    }
    pub fn include_ignored(&mut self, _: bool) -> &mut Self {
        self
    }
    pub fn include_untracked(&mut self, _: bool) -> &mut Self {
        self
    }
}
impl Version {
    pub fn get() -> Self {
        Self
    }
    pub fn libgit2_version(&self) -> (u32, u32, u32) {
        (0, 0, 0)
    }
    pub fn vendored(&self) -> bool {
        false
    }
    pub fn crate_version(&self) -> &'static str {
        "ristux-offline"
    }
}

pub struct Statuses;
pub struct StatusEntry;
impl Statuses {
    pub fn iter(&self) -> std::iter::Empty<StatusEntry> {
        std::iter::empty()
    }
}
impl StatusEntry {
    pub fn path(&self) -> Option<&str> {
        None
    }
    pub fn status(&self) -> Status {
        Status::CURRENT
    }
}

pub struct Branch {
    target: Oid,
}
pub struct Reference {
    target: Option<Oid>,
}
pub struct Commit<'a> {
    repo: &'a Repository,
    id: Oid,
}
pub struct Tag;
pub struct TreeEntry {
    id: Oid,
}
pub struct Blob(Vec<u8>);
pub struct Progress<'a>(std::marker::PhantomData<&'a ()>);
impl Progress<'_> {
    pub fn indexed_deltas(&self) -> usize {
        0
    }
    pub fn total_deltas(&self) -> usize {
        0
    }
    pub fn received_bytes(&self) -> usize {
        0
    }
    pub fn indexed_objects(&self) -> usize {
        0
    }
    pub fn total_objects(&self) -> usize {
        0
    }
}
impl Branch {
    pub fn get(&self) -> Reference {
        Reference {
            target: Some(self.target),
        }
    }
}
impl Reference {
    pub fn target(&self) -> Option<Oid> {
        self.target
    }
    pub fn peel(&self, _: ObjectType) -> Result<Object<'static>, Error> {
        Err(Error)
    }
}
impl Commit<'_> {
    pub fn id(&self) -> Oid {
        self.id
    }
    pub fn tree(&self) -> Result<Tree<'_>, Error> {
        let commit = self
            .repo
            .inner
            .find_object(self.id.into_gix())
            .map_err(|_| Error)?
            .peel_to_commit()
            .map_err(|_| Error)?;
        let tree = commit.tree().map_err(|_| Error)?;
        Ok(Tree {
            repo: self.repo,
            id: Oid::from_gix(tree.id),
        })
    }
}
impl Tag {
    pub fn target_id(&self) -> Oid {
        Oid([0; 20])
    }
}
impl Object<'_> {
    pub fn id(&self) -> Oid {
        self.id
    }
    pub fn peel(&self, _: ObjectType) -> Result<Object<'_>, Error> {
        let commit = self
            .repo
            .inner
            .find_object(self.id.into_gix())
            .map_err(|_| Error)?
            .peel_to_commit()
            .map_err(|_| Error)?;
        Ok(Object {
            repo: self.repo,
            id: Oid::from_gix(commit.id),
            blob: None,
        })
    }
    pub fn short_id(&self) -> Result<Buf, Error> {
        Ok(Buf(self.id.to_string()[..7].into()))
    }
    pub fn as_tag(&self) -> Option<&Tag> {
        None
    }
    pub fn as_blob(&self) -> Option<&Blob> {
        self.blob.as_ref()
    }
}
impl Tree<'_> {
    pub fn get_path(&self, path: &std::path::Path) -> Result<TreeEntry, Error> {
        let tree = self
            .repo
            .inner
            .find_object(self.id.into_gix())
            .map_err(|_| Error)?
            .peel_to_tree()
            .map_err(|_| Error)?;
        let entry = tree
            .lookup_entry_by_path(path)
            .map_err(|_| Error)?
            .ok_or(Error)?;
        Ok(TreeEntry {
            id: Oid::from_gix(entry.id().detach()),
        })
    }
}
impl TreeEntry {
    pub fn id(&self) -> Oid {
        self.id
    }
    pub fn to_object<'a>(&self, repo: &'a Repository) -> Result<Object<'a>, Error> {
        object_from_oid(repo, self.id)
    }
}
impl Blob {
    pub fn content(&self) -> &[u8] {
        &self.0
    }
}

fn object_from_oid(repo: &Repository, id: Oid) -> Result<Object<'_>, Error> {
    let object = repo.inner.find_object(id.into_gix()).map_err(|_| Error)?;
    let blob = object
        .try_into_blob()
        .ok()
        .map(|blob| Blob(blob.data.clone()));
    Ok(Object { repo, id, blob })
}

fn clear_workdir(path: &Path) -> Result<(), Error> {
    for entry in std::fs::read_dir(path).map_err(|_| Error)? {
        let entry = entry.map_err(|_| Error)?;
        if entry.file_name() == ".git" {
            continue;
        }
        let path = entry.path();
        let metadata = std::fs::symlink_metadata(&path).map_err(|_| Error)?;
        if metadata.is_dir() && !metadata.file_type().is_symlink() {
            std::fs::remove_dir_all(path).map_err(|_| Error)?;
        } else {
            std::fs::remove_file(path).map_err(|_| Error)?;
        }
    }
    Ok(())
}

fn checkout_tree(repo: &Repository, id: Oid) -> Result<(), Error> {
    use gix::object::tree::EntryKind;
    #[cfg(unix)]
    use std::os::unix::ffi::OsStrExt;
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    let workdir = repo.workdir().ok_or(Error)?;
    clear_workdir(workdir)?;
    let tree_id = repo
        .inner
        .find_object(id.into_gix())
        .map_err(|_| Error)?
        .peel_to_commit()
        .map_err(|_| Error)?
        .tree_id()
        .map_err(|_| Error)?
        .detach();
    let (mut stream, _) = repo.inner.worktree_stream(tree_id).map_err(|_| Error)?;
    while let Some(mut entry) = stream.next_entry().map_err(|_| Error)? {
        let relative = gix::path::from_bstr(entry.relative_path()).to_path_buf();
        let destination = workdir.join(relative);
        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent).map_err(|_| Error)?;
        }
        let mut data = Vec::with_capacity(entry.bytes_remaining().unwrap_or(0));
        entry.read_to_end(&mut data).map_err(|_| Error)?;
        match entry.mode.kind() {
            EntryKind::Blob | EntryKind::BlobExecutable => {
                let mut file = std::fs::File::create(&destination).map_err(|_| Error)?;
                file.write_all(&data).map_err(|_| Error)?;
                #[cfg(unix)]
                if entry.mode.kind() == EntryKind::BlobExecutable {
                    std::fs::set_permissions(&destination, std::fs::Permissions::from_mode(0o755))
                        .map_err(|_| Error)?;
                }
            }
            EntryKind::Link => {
                #[cfg(unix)]
                std::os::unix::fs::symlink(std::ffi::OsStr::from_bytes(&data), &destination)
                    .map_err(|_| Error)?;
                #[cfg(not(unix))]
                std::fs::write(&destination, data).map_err(|_| Error)?;
            }
            EntryKind::Tree | EntryKind::Commit => {}
        }
    }
    std::fs::write(repo.git_dir.join("HEAD"), format!("{}\n", id)).map_err(|_| Error)?;
    Ok(())
}

impl Repository {
    fn from_gix(inner: gix::Repository) -> Self {
        let git_dir = inner.path().to_path_buf();
        let workdir = inner.workdir().map(Path::to_path_buf);
        Self {
            inner,
            git_dir,
            workdir,
        }
    }

    pub fn discover<P: AsRef<std::path::Path>>(path: P) -> Result<Self, Error> {
        gix::discover(path).map(Self::from_gix).map_err(|_| Error)
    }
    pub fn open<P: AsRef<std::path::Path>>(path: P) -> Result<Self, Error> {
        gix::open(path.as_ref().to_path_buf())
            .map(Self::from_gix)
            .map_err(|_| Error)
    }
    pub fn init<P: AsRef<std::path::Path>>(path: P) -> Result<Self, Error> {
        gix::init(path).map(Self::from_gix).map_err(|_| Error)
    }
    pub fn init_opts<P: AsRef<std::path::Path>>(
        path: P,
        opts: &RepositoryInitOptions,
    ) -> Result<Self, Error> {
        if opts.bare {
            gix::init_bare(path).map(Self::from_gix).map_err(|_| Error)
        } else {
            Self::init(path)
        }
    }
    pub fn statuses(&self, _: Option<&mut StatusOptions>) -> Result<Statuses, Error> {
        Ok(Statuses)
    }
    pub fn workdir(&self) -> Option<&std::path::Path> {
        self.workdir.as_deref()
    }
    pub fn path(&self) -> &std::path::Path {
        &self.git_dir
    }
    pub fn is_path_ignored<P: AsRef<std::path::Path>>(&self, _: P) -> Result<bool, Error> {
        Ok(false)
    }
    pub fn is_shallow(&self) -> bool {
        self.git_dir.join("shallow").exists()
    }
    pub fn config(&self) -> Result<Config, Error> {
        Ok(Config)
    }
    pub fn find_object(&self, id: Oid, _: Option<ObjectType>) -> Result<Object<'_>, Error> {
        object_from_oid(self, id)
    }
    pub fn find_commit(&self, id: Oid) -> Result<Commit<'_>, Error> {
        self.inner
            .find_object(id.into_gix())
            .map_err(|_| Error)?
            .peel_to_commit()
            .map_err(|_| Error)?;
        Ok(Commit { repo: self, id })
    }
    pub fn revparse_single(&self, spec: &str) -> Result<Object<'_>, Error> {
        let id = self.inner.rev_parse_single(spec).map_err(|_| Error)?.detach();
        let object = self.inner.find_object(id).map_err(|_| Error)?;
        let id = object
            .peel_to_commit()
            .map(|commit| commit.id)
            .unwrap_or(id);
        object_from_oid(self, Oid::from_gix(id))
    }
    pub fn refname_to_id(&self, name: &str) -> Result<Oid, Error> {
        let mut reference = self.inner.find_reference(name).map_err(|_| Error)?;
        let id = reference.peel_to_id().map_err(|_| Error)?.detach();
        Ok(Oid::from_gix(id))
    }
    pub fn find_branch(&self, name: &str, _: BranchType) -> Result<Branch, Error> {
        let target = self.refname_to_id(&format!("refs/remotes/{name}"))?;
        Ok(Branch { target })
    }
    pub fn submodules(&self) -> Result<Vec<Submodule<'_>>, Error> {
        Ok(Vec::new())
    }
    pub fn head(&self) -> Result<Reference, Error> {
        let target = self.inner.head_id().map_err(|_| Error)?.detach();
        Ok(Reference {
            target: Some(Oid::from_gix(target)),
        })
    }
    pub fn reset(
        &self,
        object: &Object<'_>,
        _: ResetType,
        _: Option<&mut build::CheckoutBuilder>,
    ) -> Result<(), Error> {
        checkout_tree(self, object.id)
    }
    pub fn remote_anonymous(&self, _: &str) -> Result<Remote<'_>, Error> {
        Err(Error)
    }
}

pub struct Remote<'a>(std::marker::PhantomData<&'a ()>);
impl Remote<'_> {
    pub fn fetch(
        &mut self,
        _: &[String],
        _: Option<&mut FetchOptions<'_>>,
        _: Option<&str>,
    ) -> Result<(), Error> {
        Err(Error)
    }
}

impl Submodule<'_> {
    pub fn init(&mut self, _: bool) -> Result<(), Error> {
        Err(Error)
    }
    pub fn url(&self) -> Option<&str> {
        None
    }
    pub fn path(&self) -> &std::path::Path {
        std::path::Path::new(".")
    }
    pub fn name(&self) -> Option<&str> {
        None
    }
    pub fn update_strategy(&self) -> SubmoduleUpdate {
        SubmoduleUpdate::None
    }
    pub fn head_id(&self) -> Option<Oid> {
        None
    }
    pub fn open(&self) -> Result<Repository, Error> {
        Err(Error)
    }
}

impl CredentialHelper {
    pub fn new(_: &str) -> Self {
        Self { username: None }
    }
    pub fn config(&mut self, _: &Config) -> &mut Self {
        self
    }
    pub fn execute(&self) -> Option<(String, String)> {
        None
    }
}
impl Cred {
    pub fn ssh_key_from_agent(_: &str) -> Result<Self, Error> {
        Err(Error)
    }
    pub fn credential_helper(_: &Config, _: &str, _: Option<&str>) -> Result<Self, Error> {
        Err(Error)
    }
    pub fn default() -> Result<Self, Error> {
        Err(Error)
    }
    pub fn username(_: &str) -> Result<Self, Error> {
        Err(Error)
    }
}

pub mod build {
    pub struct CheckoutBuilder;
    pub enum CloneLocal {
        Local,
    }
    pub struct RepoBuilder;
    impl CheckoutBuilder {
        pub fn new() -> Self {
            Self
        }
        pub fn dry_run(&mut self) -> &mut Self {
            self
        }
        pub fn force(&mut self) -> &mut Self {
            self
        }
        pub fn progress<F>(&mut self, _: F) -> &mut Self
        where
            F: FnMut(Option<&std::path::Path>, usize, usize),
        {
            self
        }
    }
    impl RepoBuilder {
        pub fn new() -> Self {
            Self
        }
        pub fn clone_local(&mut self, _: CloneLocal) -> &mut Self {
            self
        }
        pub fn with_checkout(&mut self, _: CheckoutBuilder) -> &mut Self {
            self
        }
        pub fn fetch_options(&mut self, _: crate::FetchOptions<'_>) -> &mut Self {
            self
        }
        pub fn clone<P: AsRef<std::path::Path>>(
            &mut self,
            url: &str,
            path: P,
        ) -> Result<crate::Repository, crate::Error> {
            let parsed = gix::url::parse(gix::bstr::BStr::new(url.as_bytes()))
                .map_err(|_| crate::Error)?;
            if parsed.scheme == gix::url::Scheme::File {
                let source = gix::path::from_bstr(&parsed.path);
                let repo = gix::init(path.as_ref()).map_err(|_| crate::Error)?;
                let alternates = repo.path().join("objects/info/alternates");
                let objects = std::fs::canonicalize(source.join("objects"))
                    .map_err(|_| crate::Error)?;
                std::fs::write(alternates, format!("{}\n", objects.display()))
                    .map_err(|_| crate::Error)?;
                return Ok(crate::Repository::from_gix(repo));
            }
            let interrupt = std::sync::atomic::AtomicBool::new(false);
            let mut prepare = gix::prepare_clone(url, path.as_ref()).map_err(|_| crate::Error)?;
            let (mut checkout, _) = prepare
                .fetch_then_checkout(gix::progress::Discard, &interrupt)
                .map_err(|_| crate::Error)?;
            let (repo, _) = checkout
                .main_worktree(gix::progress::Discard, &interrupt)
                .map_err(|_| crate::Error)?;
            Ok(crate::Repository::from_gix(repo))
        }
    }
}
pub mod cert {
    pub struct Cert<'a>(std::marker::PhantomData<&'a ()>);
    pub struct CertHostkey<'a>(std::marker::PhantomData<&'a ()>);
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum SshHostKeyType {
        Rsa,
        Dss,
        Ed255219,
    }
    impl Cert<'_> {
        pub fn as_hostkey(&self) -> Option<&CertHostkey<'_>> {
            None
        }
    }
    impl CertHostkey<'_> {
        pub fn hostkey(&self) -> Option<&[u8]> {
            None
        }
        pub fn hostkey_type(&self) -> Option<SshHostKeyType> {
            None
        }
    }
    impl SshHostKeyType {
        pub fn name(&self) -> &'static str {
            match self {
                Self::Rsa => "ssh-rsa",
                Self::Dss => "ssh-dss",
                Self::Ed255219 => "ssh-ed25519",
            }
        }
        pub fn short_name(&self) -> &'static str {
            self.name()
        }
    }
}
pub mod opts {
    pub fn set_verify_owner_validation(_: bool) -> Result<(), crate::Error> {
        Ok(())
    }
}
