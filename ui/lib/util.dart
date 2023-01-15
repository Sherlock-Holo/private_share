class Util {
  static Uri getUri(String path, {Map<String, String>? query}) {
    Uri url;
    if (Uri.base.scheme == "http") {
      url = Uri.http(Uri.base.authority, path, query);
    } else {
      url = Uri.https(Uri.base.authority, path, query);
    }

    return url;
  }

  static String getWsUri(String path, {Map<String, String>? query}) {
    return getUri(path, query: query).toString().replaceFirst("http", "ws");
  }
}
