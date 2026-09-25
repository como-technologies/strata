# The registry kinds of #75: a host in AWS that serves each registry kind the example does not get
# from ECR, and a Palette registry over each. registry-kinds.sh sets registry_kinds while it runs,
# and turns it off at its end, so the baseline has none of this. registry-host.sh.tftpl lists what
# the host serves. Billable while it exists.
#
#   Palette registry      kind                                  the host name
#   <prefix>-charts       Helm, synchronized, no credentials    charts.<host>
#   <prefix>-charts-auth  Helm, with credentials                charts-auth.<host>
#   <prefix>-oci-pack     OCI, basic auth, provider type pack   oci.<host>, base path strata-pack
#   <prefix>-oci-helm     OCI, basic auth, provider type helm   oci.<host>, base path strata-helm
#   <prefix>-oci-zarf     OCI, basic auth, provider type zarf   oci.<host>, base path strata-zarf
#
# The Spectro pack registry on packs.<host> has no resource in the provider: registry-kinds.sh
# makes it and removes it through the API.

locals {
  kinds      = var.registry_kinds ? 1 : 0
  kinds_user = "strata"
  # <public IP with dashes>.sslip.io resolves to the public IP, so Caddy can get a certificate for
  # each service name with no DNS zone of the example's own.
  kinds_host = var.registry_kinds ? "${replace(aws_instance.kinds[0].public_ip, ".", "-")}.sslip.io" : ""
  kinds_tags = { Name = "${var.name_prefix}-registry-kinds", purpose = "strata-live-example", managed-by = "opentofu" }
  kinds_oci  = toset(["pack", "helm", "zarf"])
}

resource "random_password" "kinds" {
  count   = local.kinds
  length  = 24
  special = false
}

data "aws_ami" "ubuntu" {
  count       = local.kinds
  most_recent = true
  owners      = ["099720109477"]
  filter {
    name   = "name"
    values = ["ubuntu/images/hvm-ssd-gp3/ubuntu-noble-24.04-amd64-server-*"]
  }
}

resource "aws_vpc" "kinds" {
  count                = local.kinds
  cidr_block           = "10.75.0.0/16"
  enable_dns_hostnames = true
  tags                 = local.kinds_tags
}

resource "aws_internet_gateway" "kinds" {
  count  = local.kinds
  vpc_id = aws_vpc.kinds[0].id
  tags   = local.kinds_tags
}

resource "aws_subnet" "kinds" {
  count                   = local.kinds
  vpc_id                  = aws_vpc.kinds[0].id
  cidr_block              = "10.75.1.0/24"
  availability_zone       = var.az
  map_public_ip_on_launch = true
  tags                    = local.kinds_tags
}

resource "aws_route_table" "kinds" {
  count  = local.kinds
  vpc_id = aws_vpc.kinds[0].id
  route {
    cidr_block = "0.0.0.0/0"
    gateway_id = aws_internet_gateway.kinds[0].id
  }
  tags = local.kinds_tags
}

resource "aws_route_table_association" "kinds" {
  count          = local.kinds
  subnet_id      = aws_subnet.kinds[0].id
  route_table_id = aws_route_table.kinds[0].id
}

# HTTP for the certificates of Let's Encrypt, HTTPS for the registries. No SSH: the host needs no
# person.
resource "aws_security_group" "kinds" {
  count  = local.kinds
  name   = "${var.name_prefix}-registry-kinds"
  vpc_id = aws_vpc.kinds[0].id
  ingress {
    from_port   = 80
    to_port     = 80
    protocol    = "tcp"
    cidr_blocks = ["0.0.0.0/0"]
  }
  ingress {
    from_port   = 443
    to_port     = 443
    protocol    = "tcp"
    cidr_blocks = ["0.0.0.0/0"]
  }
  egress {
    from_port   = 0
    to_port     = 0
    protocol    = "-1"
    cidr_blocks = ["0.0.0.0/0"]
  }
  tags = local.kinds_tags
}

resource "aws_instance" "kinds" {
  count                  = local.kinds
  ami                    = data.aws_ami.ubuntu[0].id
  instance_type          = "t3.small"
  subnet_id              = aws_subnet.kinds[0].id
  vpc_security_group_ids = [aws_security_group.kinds[0].id]
  user_data = templatefile("${path.module}/registry-host.sh.tftpl", {
    user     = local.kinds_user
    password = random_password.kinds[0].result
  })
  # The script runs at the first boot alone: a new script needs a new instance.
  user_data_replace_on_change = true
  # A destroy removes the instance, and its public address, before the gateway: AWS refuses to
  # detach a gateway while an address of the VPC is mapped.
  depends_on = [aws_internet_gateway.kinds]
  root_block_device {
    volume_size = 20
    volume_type = "gp3"
  }
  tags = local.kinds_tags
}

# Palette checks a registry when it is created, so each name must answer over HTTPS first: the
# host installs Docker and Caddy gets its certificates after the instance starts.
resource "terraform_data" "kinds_ready" {
  count            = local.kinds
  triggers_replace = [aws_instance.kinds[0].id]
  provisioner "local-exec" {
    command = <<-EOT
      for name in charts charts-auth oci packs; do
        for i in $(seq 1 90); do
          code=$(curl -s -o /dev/null -w '%%{http_code}' "https://$name.${local.kinds_host}/v2/" || true)
          [ "$code" != 000 ] && break
          sleep 10
        done
        [ "$code" != 000 ] || { echo "https://$name.${local.kinds_host} does not answer" >&2; exit 1; }
      done
    EOT
  }
}

resource "spectrocloud_registry_helm" "kinds_charts" {
  count              = local.kinds
  name               = "${var.name_prefix}-charts"
  endpoint           = "https://charts.${local.kinds_host}"
  is_synchronization = true
  credentials {
    credential_type = "noAuth"
  }
  depends_on = [terraform_data.kinds_ready]
}

resource "spectrocloud_registry_helm" "kinds_charts_auth" {
  count              = local.kinds
  name               = "${var.name_prefix}-charts-auth"
  endpoint           = "https://charts-auth.${local.kinds_host}"
  is_synchronization = false
  credentials {
    credential_type = "basic"
    username        = local.kinds_user
    password        = random_password.kinds[0].result
  }
  depends_on = [terraform_data.kinds_ready]
}

resource "spectrocloud_registry_oci" "kinds" {
  for_each           = var.registry_kinds ? local.kinds_oci : toset([])
  name               = "${var.name_prefix}-oci-${each.key}"
  type               = "basic"
  provider_type      = each.key
  endpoint           = "https://oci.${local.kinds_host}"
  base_content_path  = "strata-${each.key}"
  is_private         = true
  is_synchronization = true
  credentials {
    credential_type = "basic"
    username        = local.kinds_user
    password        = random_password.kinds[0].result
  }
  depends_on = [terraform_data.kinds_ready]
}
