import 'package:flutter/material.dart';

class PeerInfo {
  final String peerId;
  final List<String> addrs;

  const PeerInfo(this.peerId, this.addrs);
}

class PeerList extends StatelessWidget {
  final List<PeerInfo> peers;

  const PeerList({super.key, required this.peers});

  @override
  Widget build(BuildContext context) {
    return ListView.builder(
        itemCount: peers.length,
        itemBuilder: (context, index) {
          final peer = peers[index];

          return ListTile(
            title: Text(peer.peerId),
          );
        });
  }
}
