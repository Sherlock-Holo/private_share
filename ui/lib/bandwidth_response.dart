import 'package:json_annotation/json_annotation.dart';

part 'bandwidth_response.g.dart';

@JsonSerializable()
class GetBandWidthResponse {
  final int inbound;
  final int outbound;

  GetBandWidthResponse(this.inbound, this.outbound);

  factory GetBandWidthResponse.fromJson(Map<String, dynamic> json) =>
      _$GetBandWidthResponseFromJson(json);

  Map<String, dynamic> toJson() => _$GetBandWidthResponseToJson(this);
}
