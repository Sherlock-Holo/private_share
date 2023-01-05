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
    return ListView.separated(
      itemCount: peers.length,
      itemBuilder: (context, index) {
        final peer = peers[index];

        return Card(
          child: ListTile(
            leading: const Icon(Icons.devices),
            title: Row(children: [
              const Text(
                "peer id: ",
                style: TextStyle(fontWeight: FontWeight.bold),
              ),
              Flexible(child: SelectableText(peer.peerId))
            ]),
          ),
        );
      },
      separatorBuilder: (BuildContext context, int index) => const Divider(),
    );
  }
}
