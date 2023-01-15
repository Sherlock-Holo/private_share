import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:http/http.dart' as http;
import 'package:ui/list_peers_response.dart';
import 'package:ui/util.dart';

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
                  IconButton(
                      onPressed: () async {
                        await _showAddPeerDialog();
                      },
                      icon: const Icon(Icons.add_rounded)),
                  IconButton(
                      onPressed: () {
                        setState(() {});
                      },
                      icon: const Icon(Icons.refresh_rounded)),
                ],
              ),
            ),
            Expanded(child: _createPeerListWidget())
          ],
        ));
  }

  Future<String?> _showAddPeerDialog() async {
    return showDialog(
      context: context,
      builder: (context) {
        String? peerAddr;
        final TextEditingController addPeersController =
            TextEditingController();

        return AlertDialog(
          title: const Text("Add Peer"),
          content: TextField(
            autofocus: true,
            decoration: const InputDecoration(
                labelText: "Peer",
                hintText: "/ip4/{ip_addr}/tcp/{tcp_port}/ws/p2p/{peer_id}"),
            onChanged: (value) {
              peerAddr = value;
            },
            controller: addPeersController,
          ),
          actions: [
            TextButton(
                onPressed: () {
                  Navigator.of(context).pop();
                },
                child: const Text("Cancel")),
            ValueListenableBuilder(
              valueListenable: addPeersController,
              builder: (context, value, child) {
                return TextButton(
                    onPressed: value.text.isNotEmpty
                        ? () {
                            _addPeer(peerAddr!).then((value) {
                              if (value == 200) {
                                ScaffoldMessenger.of(context)
                                    .showSnackBar(const SnackBar(
                                        content: Text(
                                  "Add peer done",
                                )));

                                setState(() {});
                              } else {
                                ScaffoldMessenger.of(context)
                                    .showSnackBar(SnackBar(
                                        content: Text(
                                  "Add peer failed: $value",
                                  style: const TextStyle(color: Colors.red),
                                )));
                              }
                            });

                            Navigator.of(context).pop();
                          }
                        : null,
                    child: const Text("OK"));
              },
            )
          ],
        );
      },
    );
  }

  Future<int> _addPeer(String peerAddr) async {
    final url = Util.getUri("/api/add_peers");
    final resp = await http.post(url,
        headers: <String, String>{
          'Content-Type': 'application/json; charset=UTF-8',
        },
        body: jsonEncode(<String, List<String>>{
          "peers": [peerAddr]
        }));

    return resp.statusCode;
  }

  Future<List<PeerInfo>> _getPeerList() async {
    final url = Util.getUri("/api/list_peers");
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
