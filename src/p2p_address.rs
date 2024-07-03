//! P2P address (`node_id@host:port`)
//!
//! This module provides the [`P2PAddress`] type and the related error types.

use core::borrow::Borrow;
use core::convert::TryFrom;
use core::str::FromStr;
use core::fmt;
#[cfg(feature = "std")]
use std::io;
#[cfg(feature = "std")]
use std::vec::Vec;
use crate::NodeId;

#[cfg(rust_v_1_77)]
use core::net;
#[cfg(not(rust_v_1_77))]
use std::net;

#[cfg(feature = "alloc")]
use alloc::{boxed::Box, string::String, borrow::ToOwned, string::ToString};

const LN_DEFAULT_PORT: u16 = 9735;

/// Abstracts over string operations.
///
/// This trait enables efficient conversions.
#[cfg(feature = "alloc")]
trait StringOps: AsRef<str> + Into<String> {
    /// Converts given range of `self` into `String`
    fn into_substring(self, start: usize, end: usize) -> String;
}

#[cfg(not(feature = "alloc"))]
trait StringOps: AsRef<str> { }

/// The implementation avoids allocations - whole point of the trait.
#[cfg(feature = "alloc")]
impl StringOps for String {
    fn into_substring(mut self, start: usize, end: usize) -> String {
        self.replace_range(0..start, "");
        self.truncate(end - start);
        self
    }
}

impl<'a> StringOps for &'a str {
    #[cfg(feature = "alloc")]
    fn into_substring(self, start: usize, end: usize) -> String {
        self[start..end].to_owned()
    }
}

/// Avoids allocations but has to store capacity
#[cfg(feature = "alloc")]
impl StringOps for Box<str> {
    fn into_substring(self, start: usize, end: usize) -> String {
        String::from(self).into_substring(start, end)
    }
}

/// Internal type that can store IP addresses without allocations.
///
/// This may be (partially) public in the future.
#[derive(Clone)]
enum HostInner {
    Ip(net::IpAddr),
    #[cfg(feature = "alloc")]
    Hostname(String),
    // TODO: onion
}

/// Type representing network address of an LN node.
///
/// This type can avoid allocations if the value is an IP address.
///
/// **Important: consumer code MUST NOT match on this using `Host { .. }` syntax.
#[derive(Clone)]
pub struct Host(HostInner);

impl Host {
    /// Returns true if it's an onion (Tor) adress.
    pub fn is_onion(&self) -> bool {
        match &self.0 {
            #[cfg(feature = "alloc")]
            HostInner::Hostname(hostname) => hostname.ends_with(".onion"),
            HostInner::Ip(_) => false,
        }
    }

    /// Returns true if it's an IP adress.
    pub fn is_ip_addr(&self) -> bool {
        match &self.0 {
            #[cfg(feature = "alloc")]
            HostInner::Hostname(_) => false,
            HostInner::Ip(_) => true,
        }
    }
}

impl fmt::Display for Host {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match &self.0 {
            HostInner::Ip(addr) => fmt::Display::fmt(&addr, f),
            #[cfg(feature = "alloc")]
            HostInner::Hostname(addr) => fmt::Display::fmt(&addr, f),
        }
    }
}

/// Helper struct that can be used to correctly display `host:port`
///
/// This is needed because IPv6 addresses need square brackets when displayed as `ip:port` but
/// square brackets are not used when they are displayed standalone.
pub struct HostPort<H: Borrow<Host>>(
    /// Host
    ///
    /// You can use `Host`, `&Host` or other smart pointers here.
    pub H,

    /// Port
    pub u16,
);

/// Makes sure to use square brackets around IPv6
impl<H: Borrow<Host>> fmt::Display for HostPort<H> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match &self.0.borrow().0 {
            HostInner::Ip(net::IpAddr::V6(addr)) => write!(f, "[{}]:{}", addr, self.1),
            _ => write!(f, "{}:{}", self.0.borrow(), self.1),
        }
    }
}

#[cfg(feature = "alloc")]
impl From<Host> for String {
    fn from(value: Host) -> Self {
        match value.0 {
            HostInner::Ip(ip_addr) => ip_addr.to_string(),
            #[cfg(feature = "alloc")]
            HostInner::Hostname(hostname) => hostname,
        }
    }
}

/// This does **not** attempt to resolve a hostname!
impl TryFrom<Host> for net::IpAddr {
    type Error = NotIpAddr;

    fn try_from(value: Host) -> Result<Self, Self::Error> {
        match value.0 {
            HostInner::Ip(ip_addr) => Ok(ip_addr),
            #[cfg(feature = "alloc")]
            HostInner::Hostname(hostname) => Err(NotIpAddr(hostname)),
        }
    }
}

/// Error returned when attempting to *convert* (not resolve) hostname to IP address.
///
/// **Important: consumer code MUST NOT match on this using `NotIpAddr { .. }` syntax.
#[cfg(feature = "alloc")]
#[derive(Debug)]
pub struct NotIpAddr(String);

/// Error returned when attempting to *convert* (not resolve) hostname to IP address.
///
/// **Important: consumer code MUST NOT match on this using `NotIpAddr { .. }` syntax.
#[derive(Debug)]
#[cfg(not(feature = "alloc"))]
#[non_exhaustive]
pub struct NotIpAddr;

impl fmt::Display for NotIpAddr {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        #[cfg(feature = "alloc")]
        {
            write!(f, "the hostname '{}' is not an IP address", self.0)
        }
        #[cfg(not(feature = "alloc"))]
        {
            write!(f, "the hostname is not an IP address")
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for NotIpAddr {}

/// Parsed Lightning P2P address.
///
/// This type stores parsed representation of P2P address usually written in form `node_id@host:port`.
/// It can efficiently parse and display the address as well as perform various conversions using
/// external crates.
///
/// It also stores host in a way that can avoid allocation if it's **not** a host name.
/// This also means it works without `alloc` feature however it can not be constructed with a
/// hostname.
///
/// **Serde limitations:** non-human-readable formats are not supported yet as it wasn't decided
/// what's the best way of doing it. Please state your preference in GitHub issues.
///
/// # Example
///
/// ```
/// # #[cfg(feature = "alloc")] {
/// let marvin_str = "029ef8ee0ba895e2807ac1df1987a7888116c468e70f42e7b089e06811b0e45482@ln-ask.me";
/// let marvin = marvin_str.parse::<ln_types::P2PAddress>().unwrap();
/// assert_eq!(marvin.node_id.to_string(), "029ef8ee0ba895e2807ac1df1987a7888116c468e70f42e7b089e06811b0e45482");
/// assert!(!marvin.host.is_ip_addr());
/// assert_eq!(marvin.port, 9735);
/// # }
/// ```
#[derive(Clone)]
pub struct P2PAddress {
    /// The representation of nodes public key
    pub node_id: NodeId,
    /// Network address of the node
    pub host: Host,
    /// Network port number of the node
    pub port: u16,
}

/// Intermediate representation of host.
///
/// This stores range representing host instead of string directly so that it can be returned from
/// a monomorphic function without requiring allocations.
enum IpOrHostnamePos {
    Ip(net::IpAddr),
    Hostname(usize, usize),
}

impl P2PAddress {
    /// Conveniently constructs [`HostPort`].
    ///
    /// This can be used when `NodeId` is not needed - e.g. when creating string representation of
    /// connection information.
    pub fn as_host_port(&self) -> HostPort<&Host> {
        HostPort(&self.host, self.port)
    }

    /// Internal monomorphic parsing method.
    ///
    /// This should improve codegen without requiring allocations.
    fn parse_raw(s: &str) -> Result<(NodeId, IpOrHostnamePos, u16), ParseErrorInner> {
        let at_pos = s.find('@').ok_or(ParseErrorInner::MissingAtSymbol)?;
        let (node_id, host_port) = s.split_at(at_pos);
        let host_port = &host_port[1..];
        let node_id = node_id.parse().map_err(ParseErrorInner::InvalidNodeId)?;
        let (host_end, port) = match (host_port.starts_with('[') && host_port.ends_with(']'), host_port.rfind(':')) {
            // The whole thing is an IPv6, without port
            (true, _) => (host_port.len(), LN_DEFAULT_PORT),
            (false, Some(pos)) => (pos, host_port[(pos + 1)..].parse().map_err(ParseErrorInner::InvalidPortNumber)?),
            (false, None) => (host_port.len(), LN_DEFAULT_PORT),
        };
        let host = &host_port[..host_end];
        let host = match host.parse::<net::Ipv4Addr>() {
            Ok(ip) => IpOrHostnamePos::Ip(ip.into()),
            // We have to explicitly parse IPv6 without port to avoid confusing `:`
            Err(_) if host.starts_with('[') && host.ends_with(']') => {
                let ip = host_port[1..(host.len() - 1)]
                    .parse::<net::Ipv6Addr>()
                    .map_err(ParseErrorInner::InvalidIpv6)?;

                IpOrHostnamePos::Ip(ip.into())
            },
            Err(_) => {
                IpOrHostnamePos::Hostname(at_pos + 1, at_pos + 1 + host_end)
            },
        };
        
        Ok((node_id, host, port))
    }

    /// Generic wrapper for parsing that is used to implement parsing from multiple types.
    fn internal_parse<S: StringOps>(s: S) -> Result<Self, ParseError> {
        let (node_id, host, port) = match Self::parse_raw(s.as_ref()) {
            Ok(result) => result,
            Err(error) => return Err(ParseError {
                #[cfg(feature = "alloc")]
                input: s.into(),
                reason: error,
            }),
        };
        let host = match host {
            #[cfg(feature = "alloc")]
            IpOrHostnamePos::Hostname(begin, end) => HostInner::Hostname(s.into_substring(begin, end)),
            #[cfg(not(feature = "alloc"))]
            IpOrHostnamePos::Hostname(_, _) => return Err(ParseError { reason: ParseErrorInner::UnsupportedHostname }),
            IpOrHostnamePos::Ip(ip) => HostInner::Ip(ip),
        };

        Ok(P2PAddress {
            node_id,
            host: Host(host),
            port,
        })
    }
}

/// Alternative formatting displays node ID in upper case
impl fmt::Display for P2PAddress {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if f.alternate() {
            write!(f, "{:X}@{}", self.node_id, HostPort(&self.host, self.port))
        } else {
            write!(f, "{:x}@{}", self.node_id, HostPort(&self.host, self.port))
        }
    }
}

/// Same as Display
impl fmt::Debug for P2PAddress {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

impl FromStr for P2PAddress {
    type Err = ParseError;

    #[inline]
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::internal_parse(s)
    }
}

impl<'a> TryFrom<&'a str> for P2PAddress {
    type Error = ParseError;

    #[inline]
    fn try_from(s: &'a str) -> Result<Self, Self::Error> {
        Self::internal_parse(s)
    }
}

#[cfg(feature = "alloc")]
impl TryFrom<String> for P2PAddress {
    type Error = ParseError;

    #[inline]
    fn try_from(s: String) -> Result<Self, Self::Error> {
        Self::internal_parse(s)
    }
}

#[cfg(feature = "alloc")]
impl TryFrom<Box<str>> for P2PAddress {
    type Error = ParseError;

    #[inline]
    fn try_from(s: Box<str>) -> Result<Self, Self::Error> {
        Self::internal_parse(s)
    }
}

/// Error returned when parsing text representation fails.
///
/// **Important: consumer code MUST NOT match on this using `ParseError { .. }` syntax.
#[derive(Debug, Clone)]
pub struct ParseError {
    #[cfg(feature = "alloc")]
    input: String,
    reason: ParseErrorInner,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        #[cfg(feature = "alloc")]
        {
            write_err!(f, "failed to parse '{}' as Lightning Network P2P address", self.input; &self.reason)
        }
        #[cfg(not(feature = "alloc"))]
        {
            write_err!(f, "failed to parse Lightning Network P2P address"; &self.reason)
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for ParseError {
    #[inline]
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        if let ParseErrorInner::InvalidNodeId(error) = &self.reason {
            Some(error)
        } else {
            Some(&self.reason)
        }
    }
}

#[derive(Debug, Clone)]
enum ParseErrorInner {
    MissingAtSymbol,
    InvalidNodeId(crate::node_id::ParseError),
    InvalidPortNumber(core::num::ParseIntError),
    InvalidIpv6(net::AddrParseError),
    #[cfg(not(feature = "alloc"))]
    UnsupportedHostname,
}

impl fmt::Display for ParseErrorInner {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ParseErrorInner::MissingAtSymbol => f.write_str("missing '@' symbol"),
            ParseErrorInner::InvalidNodeId(error) => fmt::Display::fmt(error, f),
            ParseErrorInner::InvalidPortNumber(error) => write_err!(f, "invalid port number"; error),
            ParseErrorInner::InvalidIpv6(error) => write_err!(f, "invalid IPv6 address"; error),
            #[cfg(not(feature = "alloc"))]
            ParseErrorInner::UnsupportedHostname => f.write_str("the address is a hostname which is unsupported in this build (without an allocator)"),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for ParseErrorInner {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ParseErrorInner::MissingAtSymbol => None,
            ParseErrorInner::InvalidNodeId(error) => error.source(),
            ParseErrorInner::InvalidPortNumber(error) => Some(error),
            ParseErrorInner::InvalidIpv6(error) => Some(error),
            #[cfg(not(feature = "alloc"))]
            ParseErrorInner::UnsupportedHostname => None,
        }
    }
}

/// Iterator over socket addresses returned by `to_socket_addrs()`
///
/// This is the iterator used in the implementation of [`std::net::ToSocketAddrs`] for [`HostPort`]
/// and [`P2PAddress`].
#[cfg(feature = "std")]
pub struct SocketAddrs {
    iter: core::iter::Chain<core::option::IntoIter<net::SocketAddr>, std::vec::IntoIter<net::SocketAddr>>
}

#[cfg(feature = "std")]
impl Iterator for SocketAddrs {
    type Item = net::SocketAddr;

    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next()
    }
}

/// Note that onion addresses can never be resolved, you have to use a proxy instead.
#[cfg(feature = "std")]
impl<H: Borrow<Host>> std::net::ToSocketAddrs for HostPort<H> {
    type Iter = SocketAddrs;

    fn to_socket_addrs(&self) -> io::Result<Self::Iter> {
        if self.0.borrow().is_onion() {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, ResolveOnion));
        }

        let iter = match &self.0.borrow().0 {
            HostInner::Ip(ip_addr) => Some(net::SocketAddr::new(*ip_addr, self.1)).into_iter().chain(Vec::new()),
            HostInner::Hostname(hostname) => None.into_iter().chain((hostname.as_str(), self.1).to_socket_addrs()?),
        };

        Ok(SocketAddrs {
            iter,
        })
    }
}

/// Note that onion addresses can never be resolved, you have to use a proxy instead.
#[cfg(feature = "std")]
impl std::net::ToSocketAddrs for P2PAddress {
    type Iter = SocketAddrs;

    fn to_socket_addrs(&self) -> io::Result<Self::Iter> {
        HostPort(&self.host, self.port).to_socket_addrs()
    }
}

/// Error type returned when attempting to resolve onion address.
// If this is made public it should be future-proofed like other errors.
#[derive(Debug)]
struct ResolveOnion;

impl fmt::Display for ResolveOnion {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("attempt to resolve onion address")
    }
}

#[cfg(feature = "std")]
impl std::error::Error for ResolveOnion {}

#[cfg(feature = "parse_arg")]
mod parse_arg_impl {
    use core::fmt;
    use super::P2PAddress;

    impl parse_arg::ParseArgFromStr for P2PAddress {
        fn describe_type<W: fmt::Write>(mut writer: W) -> fmt::Result {
            writer.write_str("a Lightning Network address in the form `nodeid@host:port`")
        }
    }
}

#[cfg(feature = "serde")]
mod serde_impl {
    use core::fmt;
    use super::P2PAddress;
    use serde::{Serialize, Deserialize, Serializer, Deserializer, de::{Visitor, Error}};
    use core::convert::TryInto;

    #[cfg(feature = "serde_alloc")]
    use alloc::string::String;

    struct HRVisitor;

    impl<'de> Visitor<'de> for HRVisitor {
        type Value = P2PAddress;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("a 66 digits long hex string")
        }

        fn visit_str<E>(self, v: &str) -> Result<Self::Value, E> where E: Error {
            v.try_into().map_err(|error| {
                E::custom(error)
            })
        }

        #[cfg(feature = "serde_alloc")]
        fn visit_string<E>(self, v: String) -> Result<Self::Value, E> where E: Error {
            v.try_into().map_err(|error| {
                E::custom(error)
            })
        }
    }

    /// Serialized as string to human-readable formats.
    ///
    /// # Errors
    ///
    /// This fails if the format is **not** human-readable because it's not decided how it should
    /// be done.
    #[cfg_attr(docsrs, doc(cfg(feature = "serde")))]
    impl Serialize for P2PAddress {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error> where S: Serializer {
            use serde::ser::Error;

            if serializer.is_human_readable() {
                serializer.collect_str(self)
            } else {
                Err(S::Error::custom("serialization is not yet implemented for non-human-readable formats, please file a request"))
            }
        }
    }

    /// Deserialized as string from human-readable formats.
    ///
    /// # Errors
    ///
    /// This fails if the format is **not** human-readable because it's not decided how it should
    /// be done.
    #[cfg_attr(docsrs, doc(cfg(feature = "serde")))]
    impl<'de> Deserialize<'de> for P2PAddress {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error> where D: Deserializer<'de> {
            if deserializer.is_human_readable() {
                deserializer.deserialize_str(HRVisitor)
            } else {
                Err(D::Error::custom("deserialization is not yet implemented for non-human-readable formats, please file a request"))
            }
        }
    }
}

#[cfg(feature = "postgres-types")]
mod postgres_impl {
    use alloc::boxed::Box;
    use super::P2PAddress;
    use postgres_types::{ToSql, FromSql, IsNull, Type};
    use bytes::BytesMut;
    use std::error::Error;

    /// Stores the value as text (same types as `&str`)
    #[cfg_attr(docsrs, doc(cfg(feature = "postgres-types")))]
    impl ToSql for P2PAddress {
        fn to_sql(&self, _ty: &Type, out: &mut BytesMut) -> Result<IsNull, Box<dyn Error + Send + Sync + 'static>> {
            use core::fmt::Write;

            write!(out, "{}", self).map(|_| IsNull::No).map_err(|error| Box::new(error) as _)
        }

        fn accepts(ty: &Type) -> bool {
            <&str as ToSql>::accepts(ty)
        }

        postgres_types::to_sql_checked!();
    }

    /// Retrieves the value as text (same types as `&str`)
    #[cfg_attr(docsrs, doc(cfg(feature = "postgres-types")))]
    impl<'a> FromSql<'a> for P2PAddress {
        fn from_sql(ty: &Type, raw: &'a [u8]) -> Result<Self, Box<dyn Error + Send + Sync + 'static>> {
            <&str>::from_sql(ty, raw)?.parse().map_err(|error| Box::new(error) as _)
        }

        fn accepts(ty: &Type) -> bool {
            <&str as FromSql>::accepts(ty)
        }
    }
}

/// Implementations of `slog` traits
#[cfg(feature = "slog")]
mod slog_impl {
    use super::P2PAddress;
    use slog::{Key, Value, KV, Record, Serializer};

    /// Uses `Display`
    #[cfg_attr(docsrs, doc(cfg(feature = "slog")))]
    impl Value for P2PAddress {
        fn serialize(&self, _rec: &Record, key: Key, serializer: &mut dyn Serializer) -> slog::Result {
            serializer.emit_arguments(key, &format_args!("{}", self))
        }
    }

    /// Serializes each field separately.
    ///
    /// The fields are:
    ///
    /// * `node_id` - delegates to `NodeId`
    /// * `host` - `Display`
    /// * `port` - `emit_u16`
    #[cfg_attr(docsrs, doc(cfg(feature = "slog")))]
    impl KV for P2PAddress {
        fn serialize(&self, rec: &Record, serializer: &mut dyn Serializer) -> slog::Result {
            // `Key` is a type alias but if `slog/dynamic_keys` feature is enabled it's not
            #![allow(clippy::useless_conversion)]
            self.node_id.serialize(rec, Key::from("node_id"), serializer)?;
            serializer.emit_arguments(Key::from("host"), &format_args!("{}", self.host))?;
            serializer.emit_u16(Key::from("port"), self.port)?;
            Ok(())
        }
    }

    impl_error_value!(super::ParseError);
}

#[cfg(test)]
mod tests {
    use super::P2PAddress;
    use alloc::{format, string::ToString};

    #[test]
    fn empty() {
        assert!("".parse::<P2PAddress>().is_err());
    }

    #[test]
    fn invalid_node_id() {
        assert!("@example.com".parse::<P2PAddress>().is_err());
    }

    #[test]
    fn invalid_port() {
        assert!("022345678901234567890123456789012345678901234567890123456789abcdef@example.com:foo".parse::<P2PAddress>().is_err());
    }

    #[test]
    #[cfg(feature = "alloc")]
    fn correct_hostname_no_port() {
        let input = "022345678901234567890123456789012345678901234567890123456789abcdef@example.com";
        let parsed = input.parse::<P2PAddress>().unwrap();
        let output = parsed.to_string();
        let expected = format!("{}{}", input, ":9735");
        assert_eq!(output, expected);
    }

    #[test]
    #[cfg(feature = "alloc")]
    fn correct_with_hostname_port() {
        let input = "022345678901234567890123456789012345678901234567890123456789abcdef@example.com:1234";
        let parsed = input.parse::<P2PAddress>().unwrap();
        let output = parsed.to_string();
        assert_eq!(output, input);
    }

    #[test]
    fn correct_ipv4_no_port() {
        let input = "022345678901234567890123456789012345678901234567890123456789abcdef@127.0.0.1";
        let parsed = input.parse::<P2PAddress>().unwrap();
        let output = parsed.to_string();
        let expected = format!("{}{}", input, ":9735");
        assert_eq!(output, expected);
    }

    #[test]
    fn correct_with_ipv4_port() {
        let input = "022345678901234567890123456789012345678901234567890123456789abcdef@127.0.0.1:1234";
        let parsed = input.parse::<P2PAddress>().unwrap();
        let output = parsed.to_string();
        assert_eq!(output, input);
    }

    #[test]
    fn ipv6_no_port() {
        let input = "022345678901234567890123456789012345678901234567890123456789abcdef@[::1]";
        let parsed = input.parse::<P2PAddress>().unwrap();
        let output = parsed.to_string();
        let expected = format!("{}{}", input, ":9735");
        assert_eq!(output, expected);
    }

    #[test]
    fn ipv6_with_port() {
        let input = "022345678901234567890123456789012345678901234567890123456789abcdef@[::1]:1234";
        let parsed = input.parse::<P2PAddress>().unwrap();
        let output = parsed.to_string();
        assert_eq!(output, input);
    }

    chk_err_impl! {
        parse_p2p_address_error_empty, "", P2PAddress, ["failed to parse '' as Lightning Network P2P address", "missing '@' symbol"], ["failed to parse Lightning Network P2P address", "missing '@' symbol"];
        parse_p2p_address_error_empty_node_id, "@127.0.0.1", P2PAddress, [
            "failed to parse '@127.0.0.1' as Lightning Network P2P address",
            "failed to parse '' as Lightning Network node ID",
            "invalid length (must be 66 chars)",
        ], [
            "failed to parse Lightning Network P2P address",
            "failed to parse Lightning Network node ID",
            "invalid length (must be 66 chars)",
        ];
    }
}
