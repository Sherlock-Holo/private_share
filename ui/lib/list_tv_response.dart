import 'package:json_annotation/json_annotation.dart';

part 'list_tv_response.g.dart';

@JsonSerializable()
class ListTVResponse {
  @JsonKey(name: 'friend_name')
  final String friendName;

  @JsonKey(name: 'encoded_url')
  final String encodedUrl;

  ListTVResponse(this.friendName, this.encodedUrl);

  factory ListTVResponse.fromJson(Map<String, dynamic> json) =>
      _$ListTVResponseFromJson(json);

  Map<String, dynamic> toJson() => _$ListTVResponseToJson(this);
}
