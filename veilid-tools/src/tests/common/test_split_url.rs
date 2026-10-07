use crate::*;
use std::net::IpAddr;
use std::str::FromStr;

macro_rules! assert_split_url {
    ($url:expr, $scheme:expr, $host:expr) => {
        assert_eq!(
            SplitUrl::from_str($url),
            Ok(SplitUrl::new($scheme, None, $host, None, None))
        );
    };
    ($url:expr, $scheme:expr, $host:expr, $port:expr) => {
        assert_eq!(
            SplitUrl::from_str($url),
            Ok(SplitUrl::new($scheme, None, $host, $port, None))
        );
    };
    ($url:expr, $scheme:expr, $host:expr, $port:expr, $path:expr) => {
        assert_eq!(
            SplitUrl::from_str($url),
            Ok(SplitUrl::new(
                $scheme,
                None,
                $host,
                $port,
                Some(SplitUrlPath::new(
                    $path,
                    Option::<String>::None,
                    Option::<String>::None
                ))
            ))
        );
    };
    ($url:expr, $scheme:expr, $host:expr, $port:expr, $path:expr, $frag:expr, $query:expr) => {
        assert_eq!(
            SplitUrl::from_str($url),
            Ok(SplitUrl::new(
                $scheme,
                None,
                $host,
                $port,
                Some(SplitUrlPath::new($path, $frag, $query))
            ))
        );
    };
}

macro_rules! assert_split_url_parse {
    ($url:expr) => {
        let url = $url;
        let su1 = SplitUrl::from_str(url).expect("should parse");
        assert_eq!(su1.to_string(), url);
    };
}

fn host<S: AsRef<str>>(s: S) -> SplitUrlHost {
    SplitUrlHost::Hostname(s.as_ref().to_owned())
}

fn ip<S: AsRef<str>>(s: S) -> SplitUrlHost {
    SplitUrlHost::IpAddr(IpAddr::from_str(s.as_ref()).unwrap())
}

pub fn test_split_url() {
    info!("testing split_url");

    assert_split_url!("http://foo", "http", host("foo"));
    assert_split_url!("http://foo:1234", "http", host("foo"), Some(1234));
    assert_split_url!("http://foo:1234/", "http", host("foo"), Some(1234), "");
    assert_split_url!(
        "http://foo:1234/asdf/qwer",
        "http",
        host("foo"),
        Some(1234),
        "asdf/qwer"
    );
    assert_split_url!("http://foo/", "http", host("foo"), None, "");
    assert_split_url!("http://11.2.3.144/", "http", ip("11.2.3.144"), None, "");
    assert_split_url!("http://[1111::2222]/", "http", ip("1111::2222"), None, "");
    assert_split_url!(
        "http://[1111::2222]:123/",
        "http",
        ip("1111::2222"),
        Some(123),
        ""
    );

    assert_split_url!(
        "http://foo/asdf/qwer",
        "http",
        host("foo"),
        None,
        "asdf/qwer"
    );
    assert_split_url!(
        "http://foo/asdf/qwer#3",
        "http",
        host("foo"),
        None,
        "asdf/qwer",
        Some("3"),
        Option::<String>::None
    );
    assert_split_url!(
        "http://foo/asdf/qwer?xxx",
        "http",
        host("foo"),
        None,
        "asdf/qwer",
        Option::<String>::None,
        Some("xxx")
    );
    assert_split_url!(
        "http://foo/asdf/qwer#yyy?xxx",
        "http",
        host("foo"),
        None,
        "asdf/qwer",
        Some("yyy"),
        Some("xxx")
    );
    assert_err!(SplitUrl::from_str("://asdf"));
    assert_err!(SplitUrl::from_str(""));
    assert_err!(SplitUrl::from_str("::"));
    assert_err!(SplitUrl::from_str("://:"));
    assert_err!(SplitUrl::from_str("a://:"));
    assert_err!(SplitUrl::from_str("a://:1243"));
    assert_err!(SplitUrl::from_str("a://:65536"));
    assert_err!(SplitUrl::from_str("a://:-16"));
    assert_err!(SplitUrl::from_str("a:///"));
    assert_err!(SplitUrl::from_str("a:///qwer:"));
    assert_err!(SplitUrl::from_str("a:///qwer://"));
    assert_err!(SplitUrl::from_str("a://qwer://"));
    assert_err!(SplitUrl::from_str("a://[1111::2222]:/"));
    assert_err!(SplitUrl::from_str("a://[1111::2222]:"));

    assert_split_url_parse!("sch://foo:bar@baz.com:1234/fnord#qux?zuz");
    assert_split_url_parse!("sch://foo:bar@baz.com:1234/fnord#qux");
    assert_split_url_parse!("sch://foo:bar@baz.com:1234/fnord?zuz");
    assert_split_url_parse!("sch://foo:bar@baz.com:1234/fnord/");
    assert_split_url_parse!("sch://foo:bar@baz.com:1234//");
    assert_split_url_parse!("sch://foo:bar@baz.com:1234");
    assert_split_url_parse!("sch://foo:bar@[1111::2222]:1234");
    assert_split_url_parse!("sch://foo:bar@[::]:1234");
    assert_split_url_parse!("sch://foo:bar@1.2.3.4:1234");
    assert_split_url_parse!("sch://@baz.com:1234");
    assert_split_url_parse!("sch://baz.com/asdf/asdf");
    assert_split_url_parse!("sch://baz.com/");
    assert_split_url_parse!("s://s");
}
