import 'dart:convert';

import 'package:file_picker/file_picker.dart';
import 'package:flutter/material.dart';
import 'package:http/http.dart' as http;
import 'package:http_parser/http_parser.dart';
import 'package:mime/mime.dart';
import 'package:ui/file_detail.dart';
import 'package:ui/list_file_response.dart';
import 'package:ui/list_peers_response.dart';
import 'package:ui/peer_info.dart';

class HomePage extends StatefulWidget {
  const HomePage({super.key, required this.title});

  final String title;

  @override
  State<HomePage> createState() => _HomePageState();
}

class _HomePageState extends State<HomePage> {
  List<FileDetail> files = [];
  List<PeerInfo> peers = [];

  Future<FileList> _getFileList() async {
    Uri url;
    if (Uri.base.scheme == "http") {
      url = Uri.http(Uri.base.authority, "/api/list_files");
    } else {
      url = Uri.https(Uri.base.authority, "/api/list_files");
    }

    final resp = await http.get(url);
    if (resp.statusCode != 200) {
      throw Exception("status code ${resp.statusCode} != 200");
    }

    final respBody = utf8.decode(resp.bodyBytes);
    final json = jsonDecode(respBody) as Map<String, dynamic>;
    final listResp = ListFileResponse.fromJson(json);

    return FileList(
        files: listResp.files.map((file) {
      return FileDetail(
          filename: file.filename,
          hash: file.hash,
          size: file.size,
          downloaded: file.downloaded);
    }).toList());
  }

  Future<PeerList> _getPeerList() async {
    Uri url;
    if (Uri.base.scheme == "http") {
      url = Uri.http(Uri.base.authority, "/api/list_peers");
    } else {
      url = Uri.https(Uri.base.authority, "/api/list_peers");
    }

    final resp = await http.get(url);
    if (resp.statusCode != 200) {
      throw Exception("status code ${resp.statusCode} != 200");
    }

    final respBody = utf8.decode(resp.bodyBytes);
    final json = jsonDecode(respBody) as Map<String, dynamic>;
    final listResp = ListPeersResponse.fromJson(json);

    return PeerList(
        peers: listResp.peers.map((peer) {
      return PeerInfo(peer.peer, peer.connectedAddrs);
    }).toList());
  }

  Future<int> _uploadFile() async {
    final result = await FilePicker.platform
        .pickFiles(withData: false, withReadStream: true);
    if (result == null) {
      return 0;
    }

    Uri url;
    if (Uri.base.scheme == "http") {
      url = Uri.http(Uri.base.authority, "/api/upload_file");
    } else {
      url = Uri.https(Uri.base.authority, "/api/upload_file");
    }

    final file = result.files.single;
    final mimeType = lookupMimeType(file.name);
    final contentType = mimeType != null ? MediaType.parse(mimeType) : null;
    final multipartFile = http.MultipartFile(
        file.name, file.readStream!, file.size,
        filename: file.name, contentType: contentType);

    final request = http.MultipartRequest("POST", url)
      ..files.add(multipartFile);

    return (await request.send()).statusCode;
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: Text(widget.title),
      ),
      body: Center(
          child: Row(
        children: [
          Expanded(
            flex: 2,
            child: Column(
              children: [
                ListTile(
                  title: const Text(
                    "File List",
                    style: TextStyle(fontWeight: FontWeight.bold, fontSize: 30),
                  ),
                  trailing: Row(
                    mainAxisSize: MainAxisSize.min,
                    children: [
                      IconButton(
                          onPressed: () {
                            _uploadFile().then((value) {
                              if (value == 0) {
                                return;
                              }

                              if (value != 200) {
                                ScaffoldMessenger.of(context)
                                    .showSnackBar(SnackBar(
                                        content: Text(
                                  "Upload failed: $value",
                                  style: const TextStyle(color: Colors.red),
                                )));
                              } else {
                                ScaffoldMessenger.of(context).showSnackBar(
                                    const SnackBar(
                                        content: Text("Upload done")));
                              }

                              setState(() {});
                            });
                          },
                          icon: const Icon(Icons.add)),
                      IconButton(
                          onPressed: () {
                            setState(() {});
                          },
                          icon: const Icon(Icons.refresh))
                    ],
                  ),
                ),
                Expanded(child: _createFileListWidget())
              ],
            ),
          ),
          const VerticalDivider(),
          Expanded(
              flex: 1,
              child: Column(
                children: [
                  ListTile(
                    title: const Text(
                      "Peer List",
                      style:
                          TextStyle(fontWeight: FontWeight.bold, fontSize: 30),
                    ),
                    trailing: Row(
                      mainAxisSize: MainAxisSize.min,
                      children: [
                        IconButton(
                            onPressed: () {}, icon: const Icon(Icons.add)),
                        IconButton(
                            onPressed: () {
                              setState(() {});
                            },
                            icon: const Icon(Icons.refresh)),
                      ],
                    ),
                  ),
                  Expanded(child: _createPeerListWidget())
                ],
              ))
        ],
      )),
    );
  }

  FutureBuilder<FileList> _createFileListWidget() {
    return FutureBuilder<FileList>(
      future: _getFileList(),
      builder: (BuildContext context, AsyncSnapshot<FileList> snapshot) {
        if (snapshot.hasData) {
          return snapshot.data!;
        }

        if (snapshot.hasError) {
          return Text(
            "${snapshot.error!}",
            style: const TextStyle(color: Colors.red, fontSize: 20),
          );
        }

        return const SizedBox(
          width: 60,
          height: 60,
          child: CircularProgressIndicator(),
        );
      },
    );
  }

  FutureBuilder<PeerList> _createPeerListWidget() {
    return FutureBuilder<PeerList>(
      future: _getPeerList(),
      builder: (context, snapshot) {
        if (snapshot.hasData) {
          return snapshot.data!;
        }

        if (snapshot.hasError) {
          return Text(
            "${snapshot.error!}",
            style: const TextStyle(color: Colors.red, fontSize: 20),
          );
        }

        return const SizedBox(
          width: 60,
          height: 60,
          child: CircularProgressIndicator(),
        );
      },
    );
  }
}
