module Sdkwork
  module BackendSdk
    module Models
      class CreatePlatformTargetRequest
              attr_accessor :target_key, :platform, :tech_stack, :architectures, :bundle_id, :package_name, :app_id, :bundle_name, :allowed_channels

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @target_key = attributes['targetKey']
                @platform = attributes['platform']
                @tech_stack = attributes['techStack']
                @architectures = attributes['architectures'].is_a?(Array) ? attributes['architectures'].map { |item| item } : []
                @bundle_id = attributes['bundleId']
                @package_name = attributes['packageName']
                @app_id = attributes['appId']
                @bundle_name = attributes['bundleName']
                @allowed_channels = attributes['allowedChannels'].is_a?(Array) ? attributes['allowedChannels'].map { |item| item } : []
              end

              def self.from_hash(data)
                return nil if data.nil?

                new(data)
              end

              def to_hash
                {
                  'targetKey' => @target_key,
                  'platform' => @platform,
                  'techStack' => @tech_stack,
                  'architectures' => @architectures.is_a?(Array) ? @architectures.map { |item| item } : [],
                  'bundleId' => @bundle_id,
                  'packageName' => @package_name,
                  'appId' => @app_id,
                  'bundleName' => @bundle_name,
                  'allowedChannels' => @allowed_channels.is_a?(Array) ? @allowed_channels.map { |item| item } : [],
                }
              end
            end
    end
  end
end
