import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:http/http.dart' as http;
import 'package:ui/file_detail.dart';
import 'package:ui/list_file_response.dart';
import 'package:ui/list_peers_response.dart';
import 'package:ui/peer_info.dart';

void main() {
  runApp(const MainApp());
}

class MainApp extends StatelessWidget {
  const MainApp({super.key});

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      title: 'private share',
      theme: ThemeData(
        primarySwatch: Colors.blue,
      ),
      home: const HomePage(title: 'private share'),
    );
  }
}

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
                const Text(
                  "File List",
                  style: TextStyle(fontWeight: FontWeight.bold, fontSize: 30),
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
                  const Text(
                    "Peer List",
                    style: TextStyle(fontWeight: FontWeight.bold, fontSize: 30),
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
