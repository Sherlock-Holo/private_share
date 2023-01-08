import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:http/http.dart' as http;
import 'package:ui/list_peers_response.dart';

class PeerInfo {
  final String peerId;
  final List<String> addrs;

  PeerInfo(this.peerId, this.addrs);
}

class PeerList extends StatefulWidget {
  const PeerList({super.key});

  @override
  State<StatefulWidget> createState() => _PeerListState();
}

class _PeerListState extends State<PeerList> {
  @override
  Widget build(BuildContext context) {
    return Expanded(
        flex: 1,
        child: Column(
          children: [
            ListTile(
              title: const Text(
                "Peer List",
                style: TextStyle(fontWeight: FontWeight.bold, fontSize: 30),
              ),
              trailing: Row(
                mainAxisSize: MainAxisSize.min,
                children: [
                  IconButton(onPressed: () {}, icon: const Icon(Icons.add)),
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
        ));
  }

  Future<List<PeerInfo>> _getPeerList() async {
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

    return listResp.peers
        .map((peer) => PeerInfo(peer.peer, peer.connectedAddrs))
        .toList();
  }

  FutureBuilder<List<PeerInfo>> _createPeerListWidget() {
    return FutureBuilder<List<PeerInfo>>(
      future: _getPeerList(),
      builder: (context, snapshot) {
        if (snapshot.hasData) {
          return _buildPeerList(snapshot.data!);
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

  Widget _buildPeerList(List<PeerInfo> peers) {
    return ListView.builder(
      itemCount: peers.length,
      itemBuilder: (context, index) {
        final peer = peers[index];

        List<Widget> children = [];
        if (peer.addrs.isNotEmpty) {
          children = [
            const Divider(
              thickness: 1.0,
              height: 1.0,
            ),
          ];
          children.addAll(peer.addrs.map((addr) {
            return ListTile(
              leading: const Icon(Icons.account_tree_rounded),
              title: SelectableText(addr),
            );
          }));
        }

        return Card(
          child: ExpansionTile(
            leading: const Icon(Icons.devices),
            title: SelectableText(peer.peerId),
            children: children,
          ),
        );
      },
    );
  }
}
