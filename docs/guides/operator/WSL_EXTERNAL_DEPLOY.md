# WSL Ubuntu 外部依赖 Docker 部署（外部 PostgreSQL / Redis）

本文档描述在 WSL Ubuntu 22.04 中，用 Docker 部署 `sdkwork-webserver`
三个环境（开发、测试、线上），**PostgreSQL 与 Redis 使用 Ubuntu 宿主自身安装的服务**
（外部依赖模式），不启动 compose 内置容器。

## 域名约定

| 环境 | 域名前缀 | 宿主端口 | 数据库 |
| --- | --- | --- | --- |
| 开发 (development) | `server-dev.sdkwork.com` | 13800 | sdkwork_ai_dev |
| 测试 (test) | `server-test.sdkwork.com` | 18888 | sdkwork_ai_test |
| 线上 (production) | `server.sdkwork.com` | 18080 | sdkwork_ai_prod |

支持的 Web Server 域名：仅 `sdkwork.com`

## 端口分配

| 环境 | 容器端口 | 宿主端口 | Redis DB |
| --- | --- | --- | --- |
| 开发 | 3800 | 13800 | 0 |
| 测试 | 8888 | 18888 | 1 |
| 线上 | 8080 | 18080 | 2 |

## 前置要求（WSL 宿主）

- **PostgreSQL**：`listen_addresses = '*'`（默认仅 127.0.0.1 时容器无法访问）；
  pg_hba 放行 docker 网段（172.16.0.0/12）；网关用户拥有
  同名 schema（`docker/postgres/external-schema.sql`）。
- **Redis**：`bind 0.0.0.0` 且 `protected-mode no`（无密码，或配置
  `WEBSERVER_REDIS_PASSWORD`）。
- 数据库与用户：`sdkwork_ai_dev` / `sdkwork_ai_test` / `sdkwork_ai_prod`，
  各带同名 schema。

## 快速部署（一键脚本）

```bash
# 在 WSL Ubuntu 中执行（需要 root 权限；按实际盘符调整挂载路径）
cd /mnt/<drive>/sdkwork-space/sdkwork-webserver
# 示例: cd /mnt/e/sdkwork-space/sdkwork-webserver
sudo bash deployments/docker/scripts/wsl-external-deploy.sh
```

该脚本会自动完成：
1. 确保宿主 Redis 运行并可访问
2. 为每个环境创建 PostgreSQL 数据库和用户
3. 配置 pg_hba 允许 Docker 网段访问
4. 停止已有的容器栈
5. 部署所有三个环境
6. 配置 /etc/hosts 和 nginx
7. 验证所有端点健康状态

## 分步部署

### 1. 数据库准备（每个环境一次）

```bash
# 开发环境
sudo -u postgres psql -c "CREATE DATABASE sdkwork_ai_dev OWNER sdkwork_ai_dev;"
sudo -u postgres psql -d sdkwork_ai_dev -v db=sdkwork_ai_dev \
  -v app_user=sdkwork_ai_dev -f deployments/docker/postgres/external-schema.sql

# 测试环境
sudo -u postgres psql -c "CREATE DATABASE sdkwork_ai_test OWNER sdkwork_ai_test;"
sudo -u postgres psql -d sdkwork_ai_test -v db=sdkwork_ai_test \
  -v app_user=sdkwork_ai_test -f deployments/docker/postgres/external-schema.sql

# 线上环境
sudo -u postgres psql -c "CREATE DATABASE sdkwork_ai_prod OWNER sdkwork_ai_prod;"
sudo -u postgres psql -d sdkwork_ai_prod -v db=sdkwork_ai_prod \
  -v app_user=sdkwork_ai_prod -f deployments/docker/postgres/external-schema.sql
```

### 2. 配置 Redis 访问

```bash
# 确保 Redis 监听所有接口
sudo sed -i 's/^bind 127.0.0.1 .*/bind 0.0.0.0 ::1/' /etc/redis/redis.conf
sudo sed -i 's/^protected-mode yes/protected-mode no/' /etc/redis/redis.conf
sudo systemctl restart redis-server
```

### 3. 配置 pg_hba 允许 Docker 网段

```bash
# 获取 pg_hba.conf 路径
hba_file=$(sudo -u postgres psql -tAc "SHOW hba_file;")

# 添加 Docker 网段访问规则
echo "host    all             all             172.16.0.0/12           scram-sha-256" | \
  sudo tee -a "$hba_file"
sudo systemctl reload postgresql
```

### 4. 部署环境

```bash
# 部署单个环境
bash scripts/docker/deploy-docker-environment.sh development
bash scripts/docker/deploy-docker-environment.sh test
bash scripts/docker/deploy-docker-environment.sh production

# 部署所有环境
bash scripts/docker/deploy-docker-environment.sh all
```

### 5. 配置 nginx 和 hosts

```bash
# 配置 /etc/hosts
sudo bash deployments/docker/scripts/install-wsl-hosts.sh

# 配置 nginx
sudo bash deployments/docker/scripts/install-wsl-nginx.sh
```

声明式 Web 配置位于 `deployments/webserver/`；该安装脚本会将 nginx 站点安装到
`/etc/nginx/sites-enabled/sdkwork/<domain>.conf`。

## 验证

```bash
# 直接端口访问
curl http://127.0.0.1:13800/healthz    # 开发
curl http://127.0.0.1:18888/healthz    # 测试
curl http://127.0.0.1:18080/healthz    # 线上

# 域名访问（需要 DNS 或 /etc/hosts 配置）
curl http://server-dev.sdkwork.com/healthz     # 开发
curl http://server-test.sdkwork.com/healthz    # 测试
curl http://server.sdkwork.com/healthz         # 线上
```

## 运维

```bash
# 停止单个环境
bash scripts/docker/deploy-docker-environment.sh development --down

# 停止所有环境
bash scripts/docker/deploy-docker-environment.sh all --down

# 升级：重新部署
bash scripts/docker/deploy-docker-environment.sh development --pull

# 查看容器状态
docker ps --filter "name=sdkwork-webserver"

# 查看日志
docker logs sdkwork-webserver-development
docker logs sdkwork-webserver-test
docker logs sdkwork-webserver-production
```

## 故障排除

### 容器无法连接数据库

1. 检查 PostgreSQL 是否监听正确：`sudo -u postgres psql -c "SHOW listen_addresses;"`
2. 检查 pg_hba.conf 是否允许 Docker 网段
3. 检查密码是否正确：`psql -h 127.0.0.1 -U sdkwork_ai_dev -d sdkwork_ai_dev`

### 容器无法连接 Redis

1. 检查 Redis 是否运行：`redis-cli ping`
2. 检查 Redis 绑定地址：`redis-cli CONFIG GET bind`
3. 检查保护模式：`redis-cli CONFIG GET protected-mode`

### nginx 无法代理

1. 检查 nginx 配置：`sudo nginx -t`
2. 检查端口监听：`ss -tlnp | grep :80`
3. 检查 nginx 错误日志：`sudo tail -f /var/log/nginx/sdkwork-webserver-*.error.log`
