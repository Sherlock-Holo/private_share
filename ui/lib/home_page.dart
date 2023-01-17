import 'package:flutter/material.dart';
import 'package:ui/bandwidth.dart' deferred as bandwidth;
import 'package:ui/file_detail.dart' deferred as file_detail;
import 'package:ui/peer_info.dart' deferred as peer_info;

class HomePage extends StatelessWidget {
  const HomePage({super.key, required this.title});

  final String title;

  Future<void> _loadFileListLib() async {
    await file_detail.loadLibrary();
  }

  Future<void> _loadPeerInfoLib() async {
    await peer_info.loadLibrary();
  }

  Future<void> _loadBandwidthLib() async {
    await bandwidth.loadLibrary();
  }

  Widget _buildFutureFileList() {
    return FutureBuilder(
      future: _loadFileListLib(),
      builder: (context, snapshot) {
        if (snapshot.connectionState == ConnectionState.done) {
          return file_detail.FileList();
        } else if (snapshot.hasError) {
          return Text(
            "Load file list failed: ${snapshot.error}",
            style: const TextStyle(color: Colors.red),
          );
        } else {
          return const CircularProgressIndicator();
        }
      },
    );
  }

  Widget _buildPeerList() {
    return FutureBuilder(
      future: _loadPeerInfoLib(),
      builder: (context, snapshot) {
        if (snapshot.connectionState == ConnectionState.done) {
          return peer_info.PeerList();
        } else if (snapshot.hasError) {
          return Text(
            "Load peer info failed: ${snapshot.error}",
            style: const TextStyle(color: Colors.red),
          );
        } else {
          return const CircularProgressIndicator();
        }
      },
    );
  }

  Widget _buildBandwidth() {
    return FutureBuilder(
      future: _loadBandwidthLib(),
      builder: (context, snapshot) {
        if (snapshot.connectionState == ConnectionState.done) {
          return bandwidth.Bandwidth();
        } else if (snapshot.hasError) {
          return Text(
            "Load bandwidth failed: ${snapshot.error}",
            style: const TextStyle(color: Colors.red),
          );
        } else {
          return const CircularProgressIndicator();
        }
      },
    );
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: Text(title),
      ),
      body: Center(
          child: Column(
        children: [
          Align(
            alignment: Alignment.centerLeft,
            child: _buildBandwidth(),
          ),
          Expanded(
              child: Row(
            children: [
              _buildFutureFileList(),
              const VerticalDivider(),
              _buildPeerList()
            ],
          ))
        ],
      )),
    );
  }
}
