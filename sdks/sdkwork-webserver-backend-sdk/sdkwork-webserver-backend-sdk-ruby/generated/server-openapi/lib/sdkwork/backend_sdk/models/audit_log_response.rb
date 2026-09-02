module Sdkwork
  module BackendSdk
    module Models
      class AuditLogResponse
              attr_accessor :id, :operator_id, :operator_type, :action, :target_type, :target_id, :target_uuid, :ip_address, :changes, :created_at

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @id = attributes['id']
                @operator_id = attributes['operatorId']
                @operator_type = attributes['operatorType']
                @action = attributes['action']
                @target_type = attributes['targetType']
                @target_id = attributes['targetId']
                @target_uuid = attributes['targetUuid']
                @ip_address = attributes['ipAddress']
                @changes = attributes['changes'].is_a?(Hash) ? attributes['changes'] : {}
                @created_at = attributes['createdAt']
              end

              def self.from_hash(data)
                return nil if data.nil?

                new(data)
              end

              def to_hash
                {
                  'id' => @id,
                  'operatorId' => @operator_id,
                  'operatorType' => @operator_type,
                  'action' => @action,
                  'targetType' => @target_type,
                  'targetId' => @target_id,
                  'targetUuid' => @target_uuid,
                  'ipAddress' => @ip_address,
                  'changes' => @changes,
                  'createdAt' => @created_at,
                }
              end
            end
    end
  end
end
