import 'package:flutter/material.dart';
import 'package:ui/bandwidth.dart';
import 'package:ui/file_detail.dart';
import 'package:ui/peer_info.dart';

class HomePage extends StatelessWidget {
  const HomePage({super.key, required this.title});

  final String title;

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: Text(title),
      ),
      body: Center(
          child: Column(
        children: [
          Row(
            children: const [
              Expanded(flex: 7, child: Spacer()),
              Expanded(flex: 3, child: Bandwidth())
            ],
          ),
          Expanded(
              child: Row(
            children: const [FileList(), VerticalDivider(), PeerList()],
          ))
        ],
      )),
    );
  }
}
