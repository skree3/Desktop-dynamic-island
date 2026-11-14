// Картинки, импортируемые из кода (Vite отдаёт URL).
declare module "*.png" {
  const url: string;
  export default url;
}
