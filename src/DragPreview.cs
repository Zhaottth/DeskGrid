using System.Drawing;
using System.Drawing.Imaging;
using System.Windows.Forms;

namespace DeskGrid
{
    internal sealed class DragPreview : Form
    {
        private static readonly Color BackgroundKey = Color.FromArgb(1, 2, 3);
        private readonly Bitmap icon;
        private readonly Timer follow = new Timer { Interval = 15 };
        private Point lastPointer;

        internal DragPreview(ShellIcons icons, FileEntry item, int size, ImageAttributes tint)
        {
            icon = new Bitmap(size, size);
            using (var graphics = Graphics.FromImage(icon))
                icons.Draw(graphics, item, new Rectangle(0, 0, size, size), tint);
            Text = "DeskGrid drag preview";
            FormBorderStyle = FormBorderStyle.None;
            AutoScaleMode = AutoScaleMode.None;
            StartPosition = FormStartPosition.Manual;
            ShowInTaskbar = false;
            TopMost = true;
            BackColor = BackgroundKey;
            TransparencyKey = BackgroundKey;
            Opacity = 0.85;
            ClientSize = icon.Size;
            follow.Tick += delegate { FollowPointer(); };
        }

        protected override bool ShowWithoutActivation { get { return true; } }

        protected override CreateParams CreateParams
        {
            get
            {
                var parameters = base.CreateParams;
                parameters.ExStyle |= WindowsShell.ToolWindow | WindowsShell.TransparentWindow | WindowsShell.NoActivateWindow;
                return parameters;
            }
        }

        internal void Begin()
        {
            lastPointer = Cursor.Position;
            Location = LocationFor(lastPointer);
            Show();
            follow.Start();
        }

        private Point LocationFor(Point pointer)
        {
            int offset = System.Math.Max(8, icon.Width / 3);
            return new Point(pointer.X + offset, pointer.Y + offset);
        }

        private void FollowPointer()
        {
            if ((Control.MouseButtons & MouseButtons.Left) == 0) { End(); return; }
            var pointer = Cursor.Position;
            if (pointer == lastPointer) return;
            lastPointer = pointer;
            WindowsShell.MoveOverlay(Handle, LocationFor(pointer));
        }

        internal void End()
        {
            follow.Stop();
            Hide();
        }

        protected override void OnPaint(PaintEventArgs e)
        {
            e.Graphics.DrawImageUnscaled(icon, 0, 0);
        }

        protected override void Dispose(bool disposing)
        {
            if (disposing)
            {
                follow.Stop();
                follow.Dispose();
            }
            base.Dispose(disposing);
            if (disposing) icon.Dispose();
        }
    }
}
