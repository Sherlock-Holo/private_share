import 'package:flutter/material.dart';

class PeerInfo {
  final String peerId;
  final List<String> addrs;
  bool expanded = false;

  PeerInfo(this.peerId, this.addrs);
}

class PeerList extends StatefulWidget {
  final List<PeerInfo> peers;

  const PeerList({super.key, required this.peers});

  @override
  State<StatefulWidget> createState() => _PeerListState();
}

class _PeerListState extends State<PeerList> {
  @override
  Widget build(BuildContext context) {
    return SingleChildScrollView(
      child: Container(
        child: _buildPanel(),
      ),
    );
  }

  Widget _buildPanel() {
    return ExpansionPanelList(
        expansionCallback: (panelIndex, isExpanded) {
          setState(() {
            widget.peers[panelIndex].expanded = !isExpanded;
          });
        },
        children: widget.peers.map((peer) {
          return ExpansionPanel(
              headerBuilder: (context, isExpanded) {
                return ListTile(
                  leading: const Icon(Icons.devices),
                  title: Row(children: [
                    const Text(
                      "peer id: ",
                      style: TextStyle(fontWeight: FontWeight.bold),
                    ),
                    Flexible(child: SelectableText(peer.peerId))
                  ]),
                );
              },
              isExpanded: peer.expanded,
              body: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: peer.addrs.map((addr) {
                  return ListTile(
                    leading: const Icon(Icons.account_tree_rounded),
                    title: SelectableText(addr),
                  );
                }).toList(),
              ));
        }).toList());
  }
}
